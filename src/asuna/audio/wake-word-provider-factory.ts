/**
 * Wake word saglayici fabrikasi (ASU-021).
 *
 * # Tek somut-tip noktasi
 *
 * `FakeWakeWordProvider` ve `SherpaKwsProvider`'i import etmesine **yalnizca bu
 * dosyanin** izni var (testler haric). Uygulamanin geri kalani
 * `createWakeWordProvider(config)` cagirir ve elinde sadece [`WakeWordProvider`]
 * arayuzu olur — ADR-004 exit plani (motor degistirme) tek dosyayi ilgilendirsin
 * diye (PROJECT.md Bolum 8: "Do not couple the rest of Asuna to a single
 * wake-word vendor").
 *
 * # Secim koda gomulu degil
 *
 * Saglayici `ASUNA_WAKE_WORD_PROVIDER`'dan gelir: Rust tarafinda dogrulanir,
 * `get_frontend_config` whitelist'i ile renderer'a gecer. Motor **detaylari**
 * (model dizini, esik) renderer'a hic gelmez — onlar Rust tarafinda kalir.
 */

import type { FrontendConfig } from '../config/frontend-config';

import { FakeWakeWordProvider } from './fake-wake-word-provider';
import { SherpaKwsProvider } from './sherpa-kws-provider';
import { WakeWordProviderError, type WakeWordProvider } from './wake-word-provider';

/**
 * Fabrikanin ihtiyac duydugu config alt kumesi.
 *
 * `FrontendConfig`'ten turetildi: alan adlari/degerleri iki yerde ayri ayri
 * tanimlanmaz, sozlesme degisirse burasi derleme zamaninda kirilir.
 */
export type WakeWordProviderConfig = Pick<FrontendConfig, 'wakeWordProvider' | 'wakeWord'>;

/**
 * Config'in secitigi saglayiciyi kurar. Saglayici **baslatilmaz**; `initialize()`
 * ve `start()` cagirmak (ve idle akisina baglamak) ASU-023'un isi.
 *
 * @throws {WakeWordProviderError} `unsupported_provider` — config'te tanimadigimiz
 *   bir saglayici adi varsa. Sessizce fake'e ya da no-op'a dusmeyiz: "dinliyorum"
 *   sanilan sagir bir uygulama, gorunur bir hatadan cok daha kotudur.
 */
export function createWakeWordProvider(config: WakeWordProviderConfig): WakeWordProvider {
  switch (config.wakeWordProvider) {
    case 'fake':
      return new FakeWakeWordProvider({ phrase: config.wakeWord });
    case 'sherpa-kws':
      return new SherpaKwsProvider({ phrase: config.wakeWord });
    default:
      // Tip acisindan erisilemez; dogrulanmamis bir config buraya duserse
      // (orn. Rust ile renderer surumleri ayrildi) gurultulu bicimde patlar.
      // Mesaj gelen degeri **tekrarlamaz** (FrontendConfigError ile ayni politika).
      throw new WakeWordProviderError(
        'unsupported_provider',
        '`ASUNA_WAKE_WORD_PROVIDER` taninmiyor — beklenen: `sherpa-kws` veya `fake`.',
      );
  }
}
