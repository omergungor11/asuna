//! ASU-008b spike — idle wake-word dongusu (CPU/RAM olcumu icin).
//!
//! Kullanim:
//!   kws-idle mic  <model_dir> <keywords.txt> <saniye>
//!   kws-idle loop <model_dir> <keywords.txt> <saniye> <wav>
//!
//! `mic`  : cpal ile varsayilan giris cihazi (macOS TCC mikrofon izni gerekir).
//! `loop` : mikrofon yerine bir WAV'i gercek zamanli (100ms chunk + sleep) tekrarlar.
//!          Mikrofon izni alinamadigi ortamlarda ayni islem yukunu uretir.
//!
//! Her iki modda da islem KWS tarafinda ayni: 100ms'lik f32 chunk -> accept_waveform -> decode.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sherpa_onnx::{
    KeywordSpotter, KeywordSpotterConfig, LinearResampler, VadModelConfig, VoiceActivityDetector,
    Wave,
};

fn build_config(model_dir: &PathBuf, keywords_file: String) -> KeywordSpotterConfig {
    let m = |name: &str| -> String {
        model_dir
            .join(name)
            .to_string_lossy()
            .into_owned()
    };
    let mut config = KeywordSpotterConfig::default();
    config.model_config.transducer.encoder =
        Some(m("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.transducer.decoder =
        Some(m("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.transducer.joiner =
        Some(m("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.tokens = Some(m("tokens.txt"));
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = 1;
    config.keywords_file = Some(keywords_file);
    config
}

fn spotter_loop(
    kws: &KeywordSpotter,
    rx: Receiver<Vec<f32>>,
    sample_rate: i32,
    deadline: Instant,
) -> usize {
    let stream = kws.create_stream();
    let mut detections = 0usize;
    let start = Instant::now();
    while Instant::now() < deadline {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(500)) else {
            continue;
        };
        stream.accept_waveform(sample_rate, &chunk);
        while kws.is_ready(&stream) {
            kws.decode(&stream);
            if let Some(result) = kws.get_result(&stream) {
                if !result
                    .keyword
                    .is_empty()
                {
                    detections += 1;
                    println!(
                        "[{:>7.1}s] DETECT #{detections}: {} (start={})",
                        start
                            .elapsed()
                            .as_secs_f32(),
                        result.keyword,
                        result.start_time
                    );
                    kws.reset(&stream);
                }
            }
        }
    }
    detections
}

fn run_mic(kws: &KeywordSpotter, seconds: u64) -> Result<usize, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("varsayilan giris cihazi yok")?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let sample_rate = supported
        .sample_rate()
        .0 as i32;
    let channels = supported.channels() as usize;
    eprintln!(
        "mikrofon: {} | {} Hz | {} kanal | {:?}",
        device
            .name()
            .unwrap_or_else(|_| "?".into()),
        sample_rate,
        channels,
        supported.sample_format()
    );

    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
    let err_fn = |e| eprintln!("cpal stream hatasi: {e}");
    let config: cpal::StreamConfig = supported
        .clone()
        .into();

    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let mono: Vec<f32> = data
                    .iter()
                    .step_by(channels)
                    .copied()
                    .collect();
                let _ = tx.send(mono);
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let mono: Vec<f32> = data
                    .iter()
                    .step_by(channels)
                    .map(|s| f32::from(*s) / 32768.0)
                    .collect();
                let _ = tx.send(mono);
            },
            err_fn,
            None,
        ),
        other => return Err(format!("desteklenmeyen ornek formati: {other:?}")),
    }
    .map_err(|e| format!("build_input_stream: {e}"))?;

    stream
        .play()
        .map_err(|e| format!("stream.play: {e}"))?;

    let deadline = Instant::now() + Duration::from_secs(seconds);
    let detections = spotter_loop(kws, rx, sample_rate, deadline);
    drop(stream);
    Ok(detections)
}

/// VAD-kapili varyant: cpal -> 16kHz resample -> Silero VAD -> (sadece konusma
/// segmentleri) -> KeywordSpotter. Idle'da (sessizlikte) zipformer encoder hic
/// calismaz; bedeli VAD'in kendi maliyeti + segment sonu gecikmesi.
fn run_mic_vad(kws: &KeywordSpotter, seconds: u64, vad_model: &str) -> Result<usize, String> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("varsayilan giris cihazi yok")?;
    let supported = device
        .default_input_config()
        .map_err(|e| format!("default_input_config: {e}"))?;
    let in_rate = supported
        .sample_rate()
        .0 as i32;
    let channels = supported.channels() as usize;
    eprintln!(
        "mikrofon(VAD): {} | {} Hz -> 16000 Hz | {} kanal | {:?}",
        device
            .name()
            .unwrap_or_else(|_| "?".into()),
        in_rate,
        channels,
        supported.sample_format()
    );

    let mut vad_config = VadModelConfig::default();
    vad_config.silero_vad.model = Some(vad_model.to_string());
    vad_config.silero_vad.threshold = 0.5;
    vad_config.silero_vad.min_silence_duration = 0.25;
    vad_config.silero_vad.min_speech_duration = 0.20;
    vad_config.silero_vad.window_size = 512;
    vad_config.sample_rate = 16000;
    vad_config.num_threads = 1;
    vad_config.provider = Some("cpu".to_string());
    let vad = VoiceActivityDetector::create(&vad_config, 10.0)
        .ok_or("VoiceActivityDetector olusturulamadi")?;
    let resampler =
        LinearResampler::create(in_rate, 16000).ok_or("LinearResampler olusturulamadi")?;

    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
    let err_fn = |e| eprintln!("cpal stream hatasi: {e}");
    let config: cpal::StreamConfig = supported
        .clone()
        .into();
    let stream = match supported.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(
                    data.iter()
                        .step_by(channels)
                        .copied()
                        .collect(),
                );
            },
            err_fn,
            None,
        ),
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config,
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                let _ = tx.send(
                    data.iter()
                        .step_by(channels)
                        .map(|s| f32::from(*s) / 32768.0)
                        .collect(),
                );
            },
            err_fn,
            None,
        ),
        other => return Err(format!("desteklenmeyen ornek formati: {other:?}")),
    }
    .map_err(|e| format!("build_input_stream: {e}"))?;
    stream
        .play()
        .map_err(|e| format!("stream.play: {e}"))?;

    let start = Instant::now();
    let deadline = start + Duration::from_secs(seconds);
    let mut detections = 0usize;
    let mut segments = 0usize;
    while Instant::now() < deadline {
        let Ok(chunk) = rx.recv_timeout(Duration::from_millis(500)) else {
            continue;
        };
        let resampled = resampler.resample(&chunk, false);
        vad.accept_waveform(&resampled);
        while !vad.is_empty() {
            let Some(segment) = vad.front() else {
                break;
            };
            segments += 1;
            let kws_stream = kws.create_stream();
            kws_stream.accept_waveform(16000, segment.samples());
            kws_stream.accept_waveform(16000, &vec![0.0f32; 8000]);
            kws_stream.input_finished();
            while kws.is_ready(&kws_stream) {
                kws.decode(&kws_stream);
                if let Some(result) = kws.get_result(&kws_stream) {
                    if !result
                        .keyword
                        .is_empty()
                    {
                        detections += 1;
                        println!(
                            "[{:>7.1}s] DETECT #{detections}: {}",
                            start
                                .elapsed()
                                .as_secs_f32(),
                            result.keyword
                        );
                        kws.reset(&kws_stream);
                    }
                }
            }
            vad.pop();
        }
    }
    drop(stream);
    eprintln!("VAD segment sayisi: {segments}");
    Ok(detections)
}

fn run_loop(kws: &KeywordSpotter, seconds: u64, wav: &str) -> Result<usize, String> {
    let wave = Wave::read(wav).ok_or_else(|| format!("wav okunamadi: {wav}"))?;
    let sample_rate = wave.sample_rate();
    let samples: Vec<f32> = wave
        .samples()
        .to_vec();
    let chunk = (sample_rate as usize) / 10; // 100ms
    eprintln!(
        "wav-loop: {wav} | {} Hz | {} sample ({:.1}s)",
        sample_rate,
        samples.len(),
        samples.len() as f32 / sample_rate as f32
    );

    let (tx, rx): (Sender<Vec<f32>>, Receiver<Vec<f32>>) = channel();
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let producer_deadline = deadline;
    let producer = thread::spawn(move || {
        let mut idx = 0usize;
        while Instant::now() < producer_deadline {
            let end = (idx + chunk).min(samples.len());
            let slice = samples[idx..end].to_vec();
            if tx
                .send(slice)
                .is_err()
            {
                break;
            }
            idx = if end >= samples.len() { 0 } else { end };
            thread::sleep(Duration::from_millis(100));
        }
    });

    let detections = spotter_loop(kws, rx, sample_rate, deadline);
    let _ = producer.join();
    Ok(detections)
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!("kullanim: kws-idle <mic|loop> <model_dir> <keywords.txt> <saniye> [wav]");
        return ExitCode::FAILURE;
    }
    let mode = args[1].clone();
    let model_dir = PathBuf::from(&args[2]);
    let keywords_file = args[3].clone();
    let seconds: u64 = args[4]
        .parse()
        .expect("saniye u64 olmali");

    let config = build_config(&model_dir, keywords_file);
    let Some(kws) = KeywordSpotter::create(&config) else {
        eprintln!("KeywordSpotter olusturulamadi");
        return ExitCode::FAILURE;
    };
    eprintln!("pid={} mod={mode} sure={seconds}s", std::process::id());

    let result = match mode.as_str() {
        "mic" => run_mic(&kws, seconds),
        "micvad" => {
            let Some(vad_model) = args.get(5) else {
                eprintln!("micvad modu icin silero_vad.onnx yolu gerekli");
                return ExitCode::FAILURE;
            };
            run_mic_vad(&kws, seconds, vad_model)
        }
        "loop" => {
            let Some(wav) = args.get(5) else {
                eprintln!("loop modu icin wav yolu gerekli");
                return ExitCode::FAILURE;
            };
            run_loop(&kws, seconds, wav)
        }
        other => Err(format!("bilinmeyen mod: {other}")),
    };

    match result {
        Ok(n) => {
            println!("toplam tespit: {n}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("HATA: {e}");
            ExitCode::FAILURE
        }
    }
}
