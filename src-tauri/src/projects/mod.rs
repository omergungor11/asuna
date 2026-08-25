//! Proje farkindaligi (PROJECT.md Bolum 15, Phase 4).
//!
//! Asuna hangi projede calisildigini **yalnizca acik yerel baglam
//! saglayicilari** uzerinden ogrenir. Diskin tamami taranmaz; "full filesystem
//! indexing" MVP disidir (PROJECT.md Bolum 4).
//!
//! | Modul | Sorumluluk |
//! |---|---|
//! | [`registry`] | Kayitli proje koklerinin tek kaynagi (ASU-040) |
//! | [`context`] | Kayitli bir projeden guvenli ve kisa ozet (ASU-041) |
//! | [`git_metadata`] | Salt okuma git durumu: branch, kirli/temiz, son commit'ler (ASU-042) |
//! | [`handoff`] | `.asuna/context.json` devir teslim artefakti (ASU-043) |
//! | [`view`] | Yukaridakilerin tek kod yolunda birlestigi `project_context` komutu (ASU-044) |
//!
//! # Cakisma kurali
//!
//! DB ile [`handoff`] dosyasi celisirse **DB kazanir**. Dosya kompakt bir devir
//! teslim artefaktidir, tek gercek kaynak degil (PROJECT.md Bolum 16).

pub mod context;
pub mod git_metadata;
pub mod handoff;
pub mod registry;
pub mod view;
