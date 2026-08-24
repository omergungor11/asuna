//! Sema migration'lari (ASU-029 / ADR-005 OQ-2).
//!
//! # Kurallar (ADR-005 "Migration Karari")
//!
//! 1. Migration'lar **sirali** ve **degismez**. Yayinlanmis bir `M` bir daha
//!    duzenlenmez; duzeltme yeni bir `M` ekler. Sebep: `PRAGMA user_version`
//!    zaten uygulanmis migration'lari tekrar calistirmaz — eski dosyayi
//!    degistirmek yalnizca yeni kurulumlari etkiler ve iki makineyi sessizce
//!    farkli semaya dusurur.
//! 2. Her `M::up` icin `M::down` yazilir. `down` gelistirme sirasinda geri
//!    alabilmek icindir; kullanicinin DB'sinde otomatik olarak calistirilmaz.
//! 3. Sema degisikligi `src/shared/*.ts` tip aynasiyla **ayni commit'te** gider.
//! 4. `migrations().validate()` bir birim testte kosar — bozuk SQL CI'da
//!    yakalanir, kullanicinin acilisinda degil.
//!
//! DDL'ler `.sql` dosyalarinda tutulur ve `include_str!` ile gomulur: ayni
//! metin hem Rust hem TypeScript testleri tarafindan okunabilsin diye
//! (bkz. ASU-030 tip aynasi senkron testi).

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};

use super::DbError;

/// Sirali migration tanimlari.
///
/// **Bu vektore yalnizca sona ekleme yapilir.** Araya ekleme ya da silme, daha
/// once uygulanmis surumlerin anlamini degistirir.
fn definitions() -> Vec<M<'static>> {
    // ASU-029: bootstrap altyapisi kuruldu, sema henuz tanimlanmadi.
    // `memories` + `sessions` ASU-030'da gelir.
    Vec::new()
}

/// Migration kumesi. Testler `validate()` icin bunu kullanir.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(definitions())
}

/// Migration'lari ileri yonde, idempotent uygular.
pub(super) fn apply(connection: &mut Connection) -> Result<(), DbError> {
    let definitions = definitions();

    // GECICI (ASU-029): liste bos oldugunda `rusqlite_migration::to_latest`
    // `NoMigrationsDefined` hatasi dondurur. Sema tanimlanana kadar (ASU-030)
    // "yapacak migration yok" gecerli bir durumdur ve uygulamayi hafizasiz
    // moda dusurmemeli. ASU-030'da ilk migration eklendiginde bu dal kalkar.
    if definitions.is_empty() {
        return Ok(());
    }

    Migrations::new(definitions)
        .to_latest(connection)
        .map_err(DbError::Migration)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bos sema ile acilis `user_version = 0` birakir; hata uretmez.
    #[test]
    fn applying_an_empty_migration_set_is_a_no_op() {
        let mut connection = Connection::open_in_memory().expect("bellek ici DB");
        apply(&mut connection).expect("bos migration kumesi hata uretmemeli");

        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version okunmali");
        assert_eq!(version, 0);
    }
}
