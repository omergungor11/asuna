//! Guvenlik primitifleri — path sandbox ve hassas dosya blok listesi.
//!
//! `asuna-config/security.md` Bolum 1-2'nin kod karsiligi.
//!
//! | Modul | Sorumluluk |
//! |---|---|
//! | [`blocklist`] | Hassas dosya blok listesi — **tek** liste (ASU-041/049) |
//! | [`sandbox`] | Kayitli proje koku disina cikilamayan yol cozumu (ASU-049) |
//!
//! Iki kural bu modulun sozlesmesi:
//!
//! - Blok listesi **merkezi**: hicbir tool ya da servis kendi kopyasini tutmaz
//!   (security.md: "Blok listesi merkezi bir modulde tanimli, tool'lar kendi
//!   kopyasini tutmaz"). Yeni bir kural [`blocklist`]'e eklenir, ikinci bir
//!   listeye degil.
//! - Dosyaya giden her tool [`sandbox::resolve_in_project`]'ten gecer. Bir
//!   fonksiyon `&Path` yerine [`sandbox::SandboxedPath`] aliyorsa kontrolun
//!   yapilmis oldugu tip duzeyinde okunur.

pub mod blocklist;
pub mod sandbox;
