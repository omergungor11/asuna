import { describe, expect, it } from 'vitest';

import { ASUNA_CORE_PROMPT, ASUNA_CORE_PROMPT_VERSION, buildAsunaInstructions } from './index';

/**
 * Prompt icerik testi davranis testidir: burada kirilan bir assertion "prompt kotu
 * yazildi" demez, "bir urun ilkesi prompt'tan dustu" der (PROJECT.md Bolum 11).
 */
describe('ASUNA_CORE_PROMPT', () => {
  it('bos degil ve tek kaynaktan (index) export ediliyor', () => {
    expect(ASUNA_CORE_PROMPT_VERSION).toBe('core.v1');
    expect(ASUNA_CORE_PROMPT.trim().length).toBeGreaterThan(200);
  });

  it('kimligi ve gorevi tanimliyor (Bolum 11)', () => {
    expect(ASUNA_CORE_PROMPT).toContain('You are Asuna');
    expect(ASUNA_CORE_PROMPT).toContain('think, remember, build, and finish');
    expect(ASUNA_CORE_PROMPT).toContain(
      "conversational layer over the user's projects, memories, and approved tools",
    );
  });

  it('uydurmama ilkesini tasiyor', () => {
    expect(ASUNA_CORE_PROMPT).toContain(
      'You have access only to the context and tools explicitly provided to you.',
    );
    expect(ASUNA_CORE_PROMPT).toContain('Never claim you saw a file');
    expect(ASUNA_CORE_PROMPT).toContain('Never describe a failed action as completed.');
  });

  it('tek somut sonraki adim ilkesini tasiyor (Bolum 5.5)', () => {
    expect(ASUNA_CORE_PROMPT).toContain('Prefer one concrete next step');
  });

  it('memory kullanimini durust tanimliyor', () => {
    expect(ASUNA_CORE_PROMPT).toContain('never invent memories');
    // Hatirlanan bilgi ile varsayim ayrimi.
    expect(ASUNA_CORE_PROMPT).toContain('daha önce');
    expect(ASUNA_CORE_PROMPT).toContain('varsayıyorum ki');
  });

  it('tool risk politikasini tasiyor (read-only serbest, mutasyon onayli)', () => {
    expect(ASUNA_CORE_PROMPT).toContain('read-only tools may be used when relevant');
    expect(ASUNA_CORE_PROMPT).toContain(
      'mutating or external actions require the configured approval policy',
    );
    expect(ASUNA_CORE_PROMPT).toContain('before requesting approval');
  });

  it('dil politikasini tasiyor (Turkce agirlikli + Ingilizce teknik terim)', () => {
    expect(ASUNA_CORE_PROMPT).toContain('The user speaks primarily Turkish');
    expect(ASUNA_CORE_PROMPT).toContain('English technical terminology');
  });

  it('kisa aktivasyon cevabi ve verimli is konusmasi istiyor (Bolum 9.2)', () => {
    expect(ASUNA_CORE_PROMPT).toContain('Buradayım.');
    expect(ASUNA_CORE_PROMPT).toContain('Dinliyorum.');
    expect(ASUNA_CORE_PROMPT).toContain('no long motivational speeches');
  });

  it('volatil veri gomulu degil — memory/proje/transcript enjeksiyonu prompt disinda', () => {
    // Prompt'ta calisma zamani verisi olmaz: dosya yolu, model ID, secret, tarih.
    expect(ASUNA_CORE_PROMPT).not.toMatch(/\/Users\//);
    expect(ASUNA_CORE_PROMPT).not.toMatch(/gpt-realtime/i);
    expect(ASUNA_CORE_PROMPT).not.toMatch(/sk-[A-Za-z0-9]/);
    expect(ASUNA_CORE_PROMPT).not.toMatch(/\{\{|\$\{/);
  });

  it('makul uzunlukta (PROJECT.md Bolum 39: dev prompt yok)', () => {
    const lines = ASUNA_CORE_PROMPT.split('\n');
    expect(lines.length).toBeLessThanOrEqual(70);
    expect(ASUNA_CORE_PROMPT.length).toBeLessThan(4000);
  });
});

describe('buildAsunaInstructions', () => {
  it('context verilmezse yalnizca cekirdek prompt doner (Phase 1)', () => {
    expect(buildAsunaInstructions()).toBe(ASUNA_CORE_PROMPT);
    expect(buildAsunaInstructions({})).toBe(ASUNA_CORE_PROMPT);
    expect(buildAsunaInstructions({ additionalSections: [] })).toBe(ASUNA_CORE_PROMPT);
  });

  it('ek bloklari cekirdek prompt sonrasina sirayla ekler', () => {
    const result = buildAsunaInstructions({
      additionalSections: ['# Memory\nfoo', '# Project\nbar'],
    });

    expect(result.startsWith(ASUNA_CORE_PROMPT)).toBe(true);
    expect(result).toBe(`${ASUNA_CORE_PROMPT}\n\n# Memory\nfoo\n\n# Project\nbar`);
  });

  it('bos/whitespace bloklari atar — prompt sonunda bos bolum olusmaz', () => {
    expect(buildAsunaInstructions({ additionalSections: ['', '   ', '\n'] })).toBe(
      ASUNA_CORE_PROMPT,
    );
    expect(buildAsunaInstructions({ additionalSections: ['  keep  ', ''] })).toBe(
      `${ASUNA_CORE_PROMPT}\n\nkeep`,
    );
  });

  it('cekirdek prompt sabitini mutasyona ugratmaz (cagrilar arasi saf)', () => {
    const before = ASUNA_CORE_PROMPT;
    buildAsunaInstructions({ additionalSections: ['x'] });
    expect(ASUNA_CORE_PROMPT).toBe(before);
  });
});
