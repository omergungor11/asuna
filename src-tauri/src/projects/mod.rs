//! Proje farkindaligi (PROJECT.md Bolum 15, Phase 4).
//!
//! Asuna hangi projede calisildigini **yalnizca acik yerel baglam
//! saglayicilari** uzerinden ogrenir. Diskin tamami taranmaz; "full filesystem
//! indexing" MVP disidir (PROJECT.md Bolum 4).
//!
//! | Modul | Sorumluluk |
//! |---|---|
//! | [`registry`] | Kayitli proje koklerinin tek kaynagi (ASU-040) |
//!
//! Sonraki task'lar bu modulun altina eklenir: `context` (ASU-041),
//! `git_metadata` (ASU-042), `handoff` (ASU-043).

pub mod registry;
