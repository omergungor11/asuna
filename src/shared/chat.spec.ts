/**
 * `src/shared/chat.ts` sozlesme testleri (plan-chat-shell.md WP4 — bosluk analizi).
 *
 * Bu dosya IPC **sinirini** kanitliyor: Rust'in dondugu JSON burada dogrulaniyor
 * ve renderer'in geri kalani yalnizca parse edilmis nesneyi goruyor. Kanitlanan
 * seyler:
 *
 * 1. Bozuk yanit **sessizce gecmez**: eksik alan, yanlis tip ve kume disi enum
 *    degeri `TypeError` uretir; `undefined` tasinmaz.
 * 2. Bilinmeyen alan tolere edilir (Rust bir alan eklerse eski renderer kirilmaz)
 *    ama parse edilmis nesneye **tasinmaz** — ornegin sunucu bir gun
 *    `attachment_list` icinde `content` gonderse bile dosya icerigi UI'ya sizmaz.
 *    (`asuna-chat.json`: "Eklenen dosyalarin icerigi renderer'a GERI DONMEZ".)
 * 3. Tip aynasi: `messages` / `attachments` kolonlari ve `role` / `origin` /
 *    `modality` CHECK kumeleri migration 006'nin metninden okunup parser'larla
 *    karsilastirilir (`schema-mirror.spec.ts` ile ayni disiplin — tek kaynak
 *    `.sql` dosyasidir, elle senkronize edilen ikinci bir tanim yok).
 */

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { cwd } from 'node:process';

import { describe, expect, it } from 'vitest';

import {
  parseChatAttachment,
  parseChatAttachmentList,
  parseChatMessage,
  parseChatMessageList,
  parseChatReply,
  parseConversationList,
  parseConversationStartResult,
  parseConversationSummary,
} from './chat';
import { toCamelCase } from './contract';

const NOW = '2026-08-31T10:00:00Z';

/** Bir alani **cikarilmis** kopya — "alan hic gelmedi" durumunu kurar. */
function without(record: Readonly<Record<string, unknown>>, key: string): Record<string, unknown> {
  return Object.fromEntries(Object.entries(record).filter(([name]) => name !== key));
}

const MESSAGE = {
  id: 7,
  sessionId: 3,
  role: 'assistant',
  content: 'Buradayım.',
  createdAt: NOW,
} as const;

const ATTACHMENT = {
  id: 5,
  sessionId: 3,
  messageId: null,
  fileName: 'notlar.md',
  mimeType: 'text/markdown',
  sizeBytes: 2048,
  origin: 'upload',
  createdAt: NOW,
} as const;

const CONVERSATION = {
  id: 3,
  title: 'Chat kabuğu',
  modality: 'text',
  projectId: 'asuna',
  startedAt: NOW,
  lastActivityAt: NOW,
  messageCount: 4,
} as const;

// ---------------------------------------------------------------------------
// Sema aynasi — tek kaynak migration 006
// ---------------------------------------------------------------------------

const SCHEMA = readFileSync(
  resolve(cwd(), 'src-tauri/src/db/migrations/006_conversations.up.sql'),
  'utf8',
);

/** `CREATE TABLE <ad> ( ... ) STRICT;` blogundaki kolon adlari, sirasiyla. */
function columnsOf(table: string): string[] {
  const start = SCHEMA.indexOf(`CREATE TABLE ${table} (`);
  expect(start, `\`${table}\` tablosu 006'da bulunmali`).toBeGreaterThanOrEqual(0);

  const body = SCHEMA.slice(start);
  const end = body.indexOf(') STRICT;');
  expect(end, `\`${table}\` \`) STRICT;\` ile kapanmali`).toBeGreaterThan(0);

  return body
    .slice(body.indexOf('(') + 1, end)
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0 && !line.startsWith('--'))
    .map((line) => line.split(/\s+/)[0] ?? '')
    .filter((name) => /^[a-z][a-z0-9_]*$/.test(name));
}

/** Bir CHECK kisitindaki (`... IN ('a', 'b')`) degerler. */
function valuesInCheck(marker: string): string[] {
  const start = SCHEMA.indexOf(marker);
  expect(start, `\`${marker}\` kisiti 006'da bulunmali`).toBeGreaterThanOrEqual(0);

  const rest = SCHEMA.slice(start + marker.length);
  return [...rest.slice(0, rest.indexOf(')')).matchAll(/'([^']+)'/g)].map(
    (match) => match[1] ?? '',
  );
}

describe('sema aynasi — messages/attachments <-> shared/chat.ts', () => {
  /**
   * `metadata_json` bilerek tasinmiyor (006'nin yorumu: bugun okunmuyor);
   * `content` de bilerek yok — attachment icerigi renderer'a donmez.
   */
  it('mesaj alanlari kolon adlariyla birebir (metadata_json disinda)', () => {
    const mirrored = columnsOf('messages')
      .map(toCamelCase)
      .filter((name) => name !== 'metadataJson');

    expect(Object.keys(parseChatMessage(MESSAGE)).sort()).toEqual([...mirrored].sort());
  });

  it('attachment alanlari kolon adlariyla birebir (content disinda)', () => {
    const mirrored = columnsOf('attachments')
      .map(toCamelCase)
      .filter((name) => name !== 'content');

    expect(Object.keys(parseChatAttachment(ATTACHMENT)).sort()).toEqual([...mirrored].sort());
  });

  it('role kumesi semadaki CHECK kisitiyla birebir', () => {
    const roles = valuesInCheck('CHECK (role IN (');
    expect(roles).toHaveLength(4);

    for (const role of roles) {
      expect(parseChatMessage({ ...MESSAGE, role }).role).toBe(role);
    }
    expect(() => parseChatMessage({ ...MESSAGE, role: 'developer' })).toThrow(TypeError);
  });

  it('origin kumesi semadaki CHECK kisitiyla birebir', () => {
    const origins = valuesInCheck('CHECK (origin IN (');
    expect(origins).toHaveLength(2);

    for (const origin of origins) {
      expect(parseChatAttachment({ ...ATTACHMENT, origin }).origin).toBe(origin);
    }
    expect(() => parseChatAttachment({ ...ATTACHMENT, origin: 'network' })).toThrow(TypeError);
  });

  it('modality kumesi semadaki CHECK kisitiyla birebir', () => {
    const modalities = valuesInCheck('CHECK (modality IN (');
    expect(modalities).toHaveLength(2);

    for (const modality of modalities) {
      expect(parseConversationSummary({ ...CONVERSATION, modality }).modality).toBe(modality);
    }
    expect(() => parseConversationSummary({ ...CONVERSATION, modality: 'video' })).toThrow(
      TypeError,
    );
  });
});

// ---------------------------------------------------------------------------
// Bozuk IPC yaniti
// ---------------------------------------------------------------------------

describe('parseChatMessage', () => {
  it('gecerli yaniti oldugu gibi tasir', () => {
    expect(parseChatMessage(MESSAGE)).toEqual(MESSAGE);
  });

  it('eksik alani sessizce undefined yapmaz, alan adiyla hata verir', () => {
    for (const key of ['id', 'sessionId', 'role', 'content', 'createdAt'] as const) {
      expect(() => parseChatMessage(without(MESSAGE, key)), `eksik alan: ${key}`).toThrow(
        new RegExp(`message\\.${key}`, 'u'),
      );
    }
  });

  it('yanlis tipli alani reddeder', () => {
    expect(() => parseChatMessage({ ...MESSAGE, id: '7' })).toThrow(TypeError);
    expect(() => parseChatMessage({ ...MESSAGE, content: null })).toThrow(TypeError);
    expect(() => parseChatMessage({ ...MESSAGE, createdAt: 1_756_636_800 })).toThrow(TypeError);
    // Sayi alani sonlu olmali: NaN bir kimlik degildir.
    expect(() => parseChatMessage({ ...MESSAGE, id: Number.NaN })).toThrow(TypeError);
  });

  it('nesne olmayan yaniti reddeder', () => {
    const cases: readonly (readonly [string, unknown])[] = [
      ['null', null],
      ['undefined', undefined],
      ['metin', 'mesaj'],
      ['sayi', 42],
      ['dizi', [MESSAGE]],
    ];

    for (const [label, value] of cases) {
      expect(() => parseChatMessage(value), `girdi: ${label}`).toThrow(TypeError);
    }
  });

  it('bilinmeyen alani tolere eder ama tasimaz', () => {
    const parsed = parseChatMessage({ ...MESSAGE, metadataJson: '{"a":1}', tokens: 12 });

    expect(parsed).toEqual(MESSAGE);
    expect(Object.keys(parsed)).not.toContain('metadataJson');
  });
});

describe('parseChatMessageList', () => {
  it('dizi olmayan yaniti reddeder', () => {
    for (const value of [null, {}, 'x']) {
      expect(() => parseChatMessageList(value)).toThrow(TypeError);
    }
  });

  it('tek bir bozuk satir tum listeyi dusurur — yarim liste gosterilmez', () => {
    expect(() => parseChatMessageList([MESSAGE, { ...MESSAGE, role: 'yok' }])).toThrow(TypeError);
  });
});

describe('parseChatAttachment', () => {
  it('null gecilebilir alanlar hem null hem undefined kabul eder', () => {
    const parsed = parseChatAttachment({
      ...ATTACHMENT,
      messageId: null,
      mimeType: undefined,
      sizeBytes: undefined,
    });

    expect(parsed.messageId).toBeNull();
    expect(parsed.mimeType).toBeNull();
    expect(parsed.sizeBytes).toBeNull();
  });

  it('null gecilebilir alanin yanlis tipini yine de reddeder', () => {
    expect(() => parseChatAttachment({ ...ATTACHMENT, mimeType: 0 })).toThrow(TypeError);
    expect(() => parseChatAttachment({ ...ATTACHMENT, sizeBytes: '2048' })).toThrow(TypeError);
    expect(() => parseChatAttachment({ ...ATTACHMENT, messageId: Number.POSITIVE_INFINITY })).toThrow(
      TypeError,
    );
  });

  /**
   * Guvenlik siniri: `attachment_list` icerik dondurmuyor. Bir gun donerse bile
   * parse edilmis nesne onu tasimaz — dosya icerigi UI state'ine girmez.
   */
  it('yanitta icerik gelse bile parse edilen kayda gecirmez', () => {
    const parsed = parseChatAttachment({
      ...ATTACHMENT,
      content: 'OPENAI_API_KEY=sk-proj-SIZAN',
    });

    expect(Object.keys(parsed)).not.toContain('content');
    expect(JSON.stringify(parsed)).not.toContain('sk-proj-SIZAN');
  });

  it('liste dizi olmayan yaniti reddeder', () => {
    expect(() => parseChatAttachmentList({ items: [] })).toThrow(TypeError);
    expect(parseChatAttachmentList([])).toEqual([]);
  });
});

describe('parseConversationSummary', () => {
  it('basliksiz konusmada null tasir, bos metin uydurmaz', () => {
    expect(parseConversationSummary({ ...CONVERSATION, title: null }).title).toBeNull();
    expect(parseConversationSummary({ ...CONVERSATION, projectId: null }).projectId).toBeNull();
  });

  it('siralama alanlari eksikse hata verir — bayat sira sessizce uretilmez', () => {
    for (const key of ['startedAt', 'lastActivityAt', 'messageCount'] as const) {
      expect(() => parseConversationSummary(without(CONVERSATION, key)), `eksik: ${key}`).toThrow(
        TypeError,
      );
    }
  });

  it('liste satirlarini sirayla dogrular', () => {
    expect(parseConversationList([CONVERSATION])).toEqual([CONVERSATION]);
    expect(() => parseConversationList(CONVERSATION)).toThrow(TypeError);
  });
});

describe('parseChatReply', () => {
  it('iki mesaji da dogrular', () => {
    const reply = parseChatReply({ userMessage: MESSAGE, assistantMessage: MESSAGE });

    expect(reply.userMessage).toEqual(MESSAGE);
    expect(reply.assistantMessage).toEqual(MESSAGE);
  });

  it('asistan mesaji eksik ya da bozuksa yaniti kabul etmez', () => {
    expect(() => parseChatReply({ userMessage: MESSAGE })).toThrow(TypeError);
    expect(() =>
      parseChatReply({ userMessage: MESSAGE, assistantMessage: { ...MESSAGE, content: 42 } }),
    ).toThrow(TypeError);
    expect(() => parseChatReply([MESSAGE, MESSAGE])).toThrow(TypeError);
  });
});

describe('parseConversationStartResult', () => {
  it('kaydedilen konusmanin kimligini alir', () => {
    expect(
      parseConversationStartResult({ status: 'recorded', session: { id: 42, model: 'x' } }),
    ).toEqual({ status: 'recorded', id: 42 });
  });

  it('hafiza kapaliysa nedeniyle birlikte skipped doner (hata degil)', () => {
    expect(
      parseConversationStartResult({ status: 'skipped', reason: 'memory-disabled' }),
    ).toEqual({ status: 'skipped', reason: 'memory-disabled' });
  });

  it('taninmayan ya da eksik status uydurulmaz', () => {
    for (const value of [
      { status: 'ok' },
      { status: 'recorded' },
      { status: 'recorded', session: { model: 'x' } },
      { status: 'skipped' },
      {},
    ]) {
      expect(() => parseConversationStartResult(value), JSON.stringify(value)).toThrow(TypeError);
    }
  });
});
