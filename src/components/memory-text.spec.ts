/**
 * Memory UI metin katmani testleri (ASU-036).
 *
 * Onemli olan: hicbir hata "bir seyler ters gitti"ye indirgenmez ve semadaki her
 * `kind` icin gorunur bir etiket vardir.
 */

import { describe, expect, it } from 'vitest';

import { MEMORY_KINDS } from '../shared/memory';
import { AsunaStoreError } from '../shared/store-error';

import {
  MEMORY_KIND_LABELS,
  describeMemoryError,
  describeMemorySource,
  formatMemoryTimestamp,
} from './memory-text';

describe('MEMORY_KIND_LABELS', () => {
  it('semadaki her kind icin bos olmayan etiket tasir', () => {
    for (const kind of MEMORY_KINDS) {
      expect(MEMORY_KIND_LABELS[kind].length).toBeGreaterThan(0);
    }
  });

  it('fazladan etiket icermez', () => {
    expect(Object.keys(MEMORY_KIND_LABELS)).toHaveLength(MEMORY_KINDS.length);
  });
});

describe('formatMemoryTimestamp', () => {
  it('gecerli zaman damgasini sabit bicimde yazar', () => {
    expect(formatMemoryTimestamp('2026-08-20T09:30:00Z')).toMatch(
      /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}$/,
    );
  });

  it('cozumlenemeyen degeri uydurmaz, oldugu gibi birakir', () => {
    expect(formatMemoryTimestamp('bozuk-tarih')).toBe('bozuk-tarih');
  });
});

describe('describeMemorySource', () => {
  it('kaynak oturumu numarasiyla soyler', () => {
    expect(describeMemorySource(7)).toBe('Oturum #7');
  });

  it('bilinmeyen kaynagi gizlemez', () => {
    expect(describeMemorySource(null)).toBe('Kaynak oturum bilinmiyor');
  });
});

describe('describeMemoryError', () => {
  it('"kullanilamiyor" ile "gecersiz istek"i ayirir', () => {
    expect(describeMemoryError(new AsunaStoreError('unavailable', 'db kilitli'))).toBe(
      'Hafıza kullanılamıyor: db kilitli',
    );
    expect(describeMemoryError(new AsunaStoreError('invalid', 'kind bilinmiyor'))).toBe(
      'İstek reddedildi: kind bilinmiyor',
    );
  });

  it('depolama ve bulunamadi hatalarinin orijinal mesajini korur', () => {
    expect(describeMemoryError(new AsunaStoreError('storage', 'disk dolu'))).toContain(
      'disk dolu',
    );
    expect(describeMemoryError(new AsunaStoreError('not-found', 'id=3'))).toContain('id=3');
  });

  it('taninmayan kodda mesaji oldugu gibi gecirir', () => {
    expect(describeMemoryError(new AsunaStoreError('unknown', 'ACL reddi'))).toBe('ACL reddi');
  });

  it('duz Error ve tamamen bilinmeyen deger icin de bir cumle uretir', () => {
    expect(describeMemoryError(new Error('beklenmedik'))).toBe('beklenmedik');
    expect(describeMemoryError(42)).toBe(
      'Hafıza işlemi bilinmeyen bir nedenle başarısız oldu.',
    );
  });
});
