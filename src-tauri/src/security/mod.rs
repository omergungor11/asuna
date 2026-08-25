//! Guvenlik primitifleri — path sandbox ve hassas dosya blok listesi.
//!
//! `asuna-config/security.md` Bolum 1-2'nin kod karsiligi. Blok listesi
//! **merkezi**: hicbir tool ya da servis kendi kopyasini tutmaz (security.md:
//! "Blok listesi merkezi bir modulde tanimli, tool'lar kendi kopyasini
//! tutmaz").

pub mod blocklist;
