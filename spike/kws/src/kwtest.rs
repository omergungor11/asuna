//! ASU-008b — keyword'lerin CALISMA ZAMANINDA duz metinden verilip verilemeyecegi testi.
//!
//! Uc yol denenir:
//!   1. keywords_file = onceden tokenlanmis dosya            (bilinen calisan yol)
//!   2. keywords_file = duz metin + modeling_unit/bpe_vocab  (sherpa kendisi tokenlar mi?)
//!   3. create_stream_with_keywords("HEY ASUNA")             (runtime override, duz metin)
//!
//! Sonuc ASU-022'de `ASUNA_WAKE_WORD` env'inin yeniden derlemeden degistirilip
//! degistirilemeyecegini belirler.

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, Wave};

fn base_config(model_dir: &PathBuf) -> KeywordSpotterConfig {
    let m = |name: &str| -> String {
        model_dir
            .join(name)
            .to_string_lossy()
            .into_owned()
    };
    let mut c = KeywordSpotterConfig::default();
    c.model_config.transducer.encoder = Some(m("encoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    c.model_config.transducer.decoder = Some(m("decoder-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    c.model_config.transducer.joiner = Some(m("joiner-epoch-12-avg-2-chunk-16-left-64.int8.onnx"));
    c.model_config.tokens = Some(m("tokens.txt"));
    c.model_config.provider = Some("cpu".to_string());
    c.model_config.num_threads = 1;
    c.keywords_score = 2.5;
    c.keywords_threshold = 0.15;
    c
}

fn detect(kws: &KeywordSpotter, wav: &str, runtime_keywords: Option<&str>) -> Option<String> {
    let wave = Wave::read(wav)?;
    let stream = match runtime_keywords {
        Some(k) => kws.create_stream_with_keywords(k),
        None => kws.create_stream(),
    };
    stream.accept_waveform(wave.sample_rate(), wave.samples());
    stream.accept_waveform(wave.sample_rate(), &vec![0.0f32; wave.sample_rate() as usize / 2]);
    stream.input_finished();
    while kws.is_ready(&stream) {
        kws.decode(&stream);
        if let Some(r) = kws.get_result(&stream) {
            if !r
                .keyword
                .is_empty()
            {
                return Some(r.keyword);
            }
        }
    }
    None
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    if args.len() < 5 {
        eprintln!("kullanim: kws-tokentest <model_dir> <tokenized.txt> <plain.txt> <wav>");
        return ExitCode::FAILURE;
    }
    let model_dir = PathBuf::from(&args[1]);
    let tokenized = args[2].clone();
    let plain = args[3].clone();
    let wav = args[4].clone();

    // 1) onceden tokenlanmis dosya
    let mut c1 = base_config(&model_dir);
    c1.keywords_file = Some(tokenized);
    match KeywordSpotter::create(&c1) {
        Some(k) => println!("1) tokenized keywords_file  -> create=OK  detect={:?}", detect(&k, &wav, None)),
        None => println!("1) tokenized keywords_file  -> create=BASARISIZ"),
    }

    // 2) duz metin dosya + modeling_unit=bpe + bpe_vocab
    // NOT: bu yol basarisiz oldugunda sherpa process'i ABORT ediyor; bu yuzden
    // atlanabilir yapildi (SKIP_CASE2=1).
    if env::var_os("SKIP_CASE2").is_none() {
    let mut c2 = base_config(&model_dir);
    c2.keywords_file = Some(plain.clone());
    c2.model_config.modeling_unit = Some("bpe".to_string());
    c2.model_config.bpe_vocab = Some(
        model_dir
            .join("bpe.vocab")
            .to_string_lossy()
            .into_owned(),
    );
    match KeywordSpotter::create(&c2) {
        Some(k) => println!("2) plain + bpe_vocab        -> create=OK  detect={:?}", detect(&k, &wav, None)),
        None => println!("2) plain + bpe_vocab        -> create=BASARISIZ"),
    }
    }

    // 3) runtime override, duz metin
    let mut c3 = base_config(&model_dir);
    c3.keywords_file = Some(args[2].clone());
    if let Some(k) = KeywordSpotter::create(&c3) {
        println!(
            "3a) create_stream_with_keywords(TOKENLANMIS) -> detect={:?}",
            detect(&k, &wav, Some("▁HE Y ▁AS ▁SO ON"))
        );
        if env::var_os("SKIP_CASE3B").is_none() {
            println!(
                "3b) create_stream_with_keywords(\"HEY ASUNA\" duz metin) -> detect={:?}",
                detect(&k, &wav, Some("HEY ASUNA"))
            );
        }
    }

    ExitCode::SUCCESS
}
