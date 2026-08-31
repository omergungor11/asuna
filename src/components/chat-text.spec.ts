/**
 * `chat-text` testleri — gruplama, baslik turetme ve hata metinleri (WP3).
 *
 * Bu fonksiyonlar saf: render yok, servis yok. Gruplama **yerel gun** sinirinda
 * calisir; bu kural burada sabitlenir, aksi halde gece yarisina yakin yazilmis
 * bir konusma yanlis basligin altina duser.
 */

import { describe, expect, it } from 'vitest';

import type { ChatAttachment, ConversationSummary } from '../shared/chat';
import { AsunaStoreError } from '../shared/store-error';

import {
  TEXT_DELETE_WARNING,
  UNTITLED_CONVERSATION,
  VOICE_CONVERSATION_FALLBACK,
  VOICE_DELETE_WARNING,
  chatErrorNotice,
  conversationGroupOf,
  conversationTitleOf,
  describeAttachment,
  describeChatError,
  describeDeleteConfirmation,
  deriveConversationTitle,
  groupConversations,
} from './chat-text';

const NOW = new Date('2026-08-31T12:00:00');

function conversation(overrides: Partial<ConversationSummary> = {}): ConversationSummary {
  return {
    id: 1,
    title: null,
    modality: 'text',
    projectId: null,
    startedAt: '2026-08-31T09:00:00',
    lastActivityAt: '2026-08-31T09:00:00',
    messageCount: 0,
    ...overrides,
  };
}

describe('conversationTitleOf', () => {
  it('baslik yoksa "Adsız konuşma" doner', () => {
    expect(conversationTitleOf(conversation())).toBe(UNTITLED_CONVERSATION);
    expect(conversationTitleOf(conversation({ title: '   ' }))).toBe(UNTITLED_CONVERSATION);
  });

  it('baslik varsa oldugu gibi doner', () => {
    expect(conversationTitleOf(conversation({ title: 'Kabuk pivotu' }))).toBe('Kabuk pivotu');
  });

  /**
   * Review H1: ses oturumu "Adsız konuşma" gorunurse kullanici onu bos bir
   * metin sohbeti sanip silebilir — silme, oturumun ozetini ve diskteki
   * dokumunu de goturur. Etiket turu soylemek zorunda.
   */
  it('basliksiz SES oturumunu metin konusmasindan ayirir', () => {
    expect(conversationTitleOf(conversation({ modality: 'voice' }))).toBe(
      VOICE_CONVERSATION_FALLBACK,
    );
    expect(conversationTitleOf(conversation({ modality: 'voice' }))).not.toBe(
      UNTITLED_CONVERSATION,
    );
  });
});

describe('describeDeleteConfirmation', () => {
  it('ses oturumunda ozet ve dokum kaybini yazar', () => {
    const text = describeDeleteConfirmation(conversation({ modality: 'voice' }));

    expect(text).toBe(VOICE_DELETE_WARNING);
    expect(text).toContain('döküm');
    expect(text).toContain('özet');
  });

  it('metin konusmasinda ses uyarisini kullanmaz', () => {
    const text = describeDeleteConfirmation(conversation({ modality: 'text' }));

    expect(text).toBe(TEXT_DELETE_WARNING);
    expect(text).not.toContain('döküm');
  });
});

describe('deriveConversationTitle', () => {
  it('ilk 60 karakteri alir', () => {
    expect(deriveConversationTitle('x'.repeat(200))).toHaveLength(60);
  });

  it('satir sonlarini ve tekrarli bosluklari tek bosluga indirir', () => {
    expect(deriveConversationTitle('  ilk satır\n\nikinci   satır  ')).toBe(
      'ilk satır ikinci satır',
    );
  });
});

describe('conversationGroupOf', () => {
  it('yerel gun sinirina gore gruplar', () => {
    expect(conversationGroupOf('2026-08-31T23:50:00', NOW)).toBe('today');
    expect(conversationGroupOf('2026-08-31T00:01:00', NOW)).toBe('today');
    expect(conversationGroupOf('2026-08-30T23:50:00', NOW)).toBe('yesterday');
    expect(conversationGroupOf('2026-08-26T10:00:00', NOW)).toBe('week');
    expect(conversationGroupOf('2026-08-25T10:00:00', NOW)).toBe('week');
    expect(conversationGroupOf('2026-08-24T10:00:00', NOW)).toBe('older');
  });

  it('cozumlenemeyen tarihi uydurmaz, en eskiye koyar', () => {
    expect(conversationGroupOf('dun', NOW)).toBe('older');
  });
});

describe('groupConversations', () => {
  it('bos gruplari dusurur ve sirayi korur', () => {
    const groups = groupConversations(
      [
        conversation({ id: 1, lastActivityAt: '2026-08-31T11:00:00' }),
        conversation({ id: 2, lastActivityAt: '2026-08-31T08:00:00' }),
        conversation({ id: 3, lastActivityAt: '2026-07-01T08:00:00' }),
      ],
      NOW,
    );

    expect(groups.map((group) => group.id)).toEqual(['today', 'older']);
    expect(groups[0]?.conversations.map((item) => item.id)).toEqual([1, 2]);
    expect(groups[0]?.label).toBe('Bugün');
  });
});

describe('describeAttachment', () => {
  const base: ChatAttachment = {
    id: 1,
    sessionId: 1,
    messageId: null,
    fileName: 'notlar.md',
    mimeType: null,
    sizeBytes: null,
    origin: 'upload',
    createdAt: '2026-08-31T10:00:00Z',
  };

  it('boyut bilinmiyorsa uydurmaz', () => {
    expect(describeAttachment(base)).toBe('notlar.md');
  });

  it('boyutu okunabilir yazar', () => {
    expect(describeAttachment({ ...base, sizeBytes: 2048 })).toBe('notlar.md · 2 KB');
    expect(describeAttachment({ ...base, sizeBytes: 512 })).toBe('notlar.md · 512 B');
  });
});

describe('hata metinleri', () => {
  it('orijinal mesaji korur', () => {
    expect(describeChatError(new AsunaStoreError('storage', 'disk dolu'))).toContain(
      'disk dolu',
    );
    expect(describeChatError(new Error('ağ yok'))).toBe('ağ yok');
    expect(describeChatError('garip')).toBe(
      'Konuşma işlemi bilinmeyen bir nedenle başarısız oldu.',
    );
  });

  it('depo kapaliysa "tekrar dene" demez, Ayarlar’a yonlendirir', () => {
    const notice = chatErrorNotice(new AsunaStoreError('unavailable', 'hafıza kapalı'));

    expect(notice.kind).toBe('memory_unavailable');
    expect(notice.retryable).toBe(false);
    expect(notice.action).toContain('Ayarlar');
  });
});
