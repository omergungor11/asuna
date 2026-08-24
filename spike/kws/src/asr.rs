//! ASU-008b tanilama — KWS transducer'ini duz online ASR olarak calistirir.
//!
//! Amac: akustik modelin "Hey Asuna" icin GERCEKTE hangi token dizisini urettigini
//! gormek. KWS ancak keywords.txt'teki token dizisi decode edilebilirse tetiklenir;
//! dolayisiyla dogru keyword yazimini burada tespit ediyoruz.
//!
//! Kullanim: kws-asr <model_dir> <wav_dir>...

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use sherpa_onnx::{OnlineRecognizer, OnlineRecognizerConfig, Wave};

fn collect_wavs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
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
    if args.len() < 3 {
        eprintln!("kullanim: kws-asr <model_dir> <wav_dir>...");
        return ExitCode::FAILURE;
    }
    let model_dir = PathBuf::from(&args[1]);
    let m = |name: &str| -> String {
        model_dir
            .join(name)
            .to_string_lossy()
            .into_owned()
    };

    let mut config = OnlineRecognizerConfig::default();
    config.model_config.transducer.encoder =
        Some(m("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.transducer.decoder =
        Some(m("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.transducer.joiner =
        Some(m("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    config.model_config.tokens = Some(m("tokens.txt"));
    config.model_config.provider = Some("cpu".to_string());
    config.model_config.num_threads = 1;
    config.decoding_method = Some("greedy_search".to_string());

    let Some(rec) = OnlineRecognizer::create(&config) else {
        eprintln!("OnlineRecognizer olusturulamadi");
        return ExitCode::FAILURE;
    };

    let mut wavs = Vec::new();
    for d in &args[2..] {
        collect_wavs(Path::new(d), &mut wavs);
    }
    wavs.sort();

    for path in &wavs {
        let name = path.to_string_lossy();
        let Some(wave) = Wave::read(&name) else {
            continue;
        };
        let stream = rec.create_stream();
        stream.accept_waveform(wave.sample_rate(), wave.samples());
        let tail = vec![0.0f32; wave.sample_rate() as usize / 2];
        stream.accept_waveform(wave.sample_rate(), &tail);
        stream.input_finished();
        while rec.is_ready(&stream) {
            rec.decode(&stream);
        }
        let (text, tokens) = rec
            .get_result(&stream)
            .map(|r| (r.text, r.tokens.join(" ")))
            .unwrap_or_default();
        println!(
            "{}\t{}\t{}",
            path.file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            text.trim(),
            tokens
        );
    }

    ExitCode::SUCCESS
}
