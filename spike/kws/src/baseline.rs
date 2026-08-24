fn main() {
    // Sherpa REFERANS ETMEZ: statik lib boyut deltasini olcmek icin taban.
    let host = cpal::default_host();
    let dev = cpal::traits::HostTrait::default_input_device(&host);
    println!("{:?}", dev.map(|d| cpal::traits::DeviceTrait::name(&d)));
}
