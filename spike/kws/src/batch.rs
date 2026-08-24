//! ASU-008b spike — toplu WAV uzerinde "HEY ASUNA" KWS taramasi.
//!
//! Kullanim:
//!   kws-batch <model_dir> <keywords.txt> <score> <threshold> <wav_dir> [<wav_dir> ...]
//!
//! Cikti: her WAV icin `TSV` satiri -> dosya \t detected(0/1) \t keyword \t start_time \t json
//! Streaming davranisi gercek mikrofona yakin olsun diye ses 100ms'lik chunk'lar
//! halinde beslenir (16kHz'de 1600 sample).

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, Wave};

const CHUNK_SAMPLES: usize = 1600; // 16kHz'de 100ms

fn collect_wavs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        eprintln!("dizin okunamadi: {}", dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_wavs(&path, out);
        } else if path.extension().is_some_and(|e| e == "wav") {
            out.push(path);
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 6 {
        eprintln!("kullanim: kws-batch <model_dir> <keywords.txt> <score> <threshold> <wav_dir>...");
        return ExitCode::FAILURE;
    }

    let model_dir = PathBuf::from(&args[1]);
    let keywords_file = args[2].clone();
    let keywords_score: f32 = args[3].parse().expect("score f32 olmali");
    let keywords_threshold: f32 = args[4].parse().expect("threshold f32 olmali");

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
    config.keywords_score = keywords_score;
    config.keywords_threshold = keywords_threshold;

    let Some(kws) = KeywordSpotter::create(&config) else {
        eprintln!("KeywordSpotter olusturulamadi");
        return ExitCode::FAILURE;
    };

    let mut wavs: Vec<PathBuf> = Vec::new();
    for dir in &args[5..] {
        collect_wavs(Path::new(dir), &mut wavs);
    }
    wavs.sort();

    for wav_path in &wavs {
        let name = wav_path.to_string_lossy();
        let Some(wave) = Wave::read(&name) else {
            eprintln!("wav okunamadi: {name}");
            continue;
        };

        let stream = kws.create_stream();
        let samples = wave.samples();
        let sample_rate = wave.sample_rate();

        let mut hits: Vec<(String, f32, String)> = Vec::new();
        let mut feed = |chunk: &[f32]| {
            stream.accept_waveform(sample_rate, chunk);
            while kws.is_ready(&stream) {
                kws.decode(&stream);
                if let Some(result) = kws.get_result(&stream) {
                    if !result
                        .keyword
                        .is_empty()
                    {
                        hits.push((
                            result.keyword.clone(),
                            result.start_time,
                            result
                                .json
                                .replace('\n', " "),
                        ));
                        kws.reset(&stream);
                    }
                }
            }
        };

        for chunk in samples.chunks(CHUNK_SAMPLES) {
            feed(chunk);
        }
        // Kuyruk: son parcanin decode edilebilmesi icin 0.5sn sessizlik ekle
        let tail = vec![0.0f32; (sample_rate as usize) / 2];
        feed(&tail);
        stream.input_finished();
        while kws.is_ready(&stream) {
            kws.decode(&stream);
        }

        let detected = u8::from(!hits.is_empty());
        let (kw, start, json) = hits
            .first()
            .cloned()
            .unwrap_or_else(|| (String::new(), -1.0, String::new()));
        println!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            wav_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            detected,
            hits.len(),
            kw,
            start,
            json
        );
    }

    ExitCode::SUCCESS
}
