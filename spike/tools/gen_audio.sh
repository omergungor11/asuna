#!/usr/bin/env bash
# ASU-008b — macOS `say` ile 16kHz mono 16-bit WAV test korpusu.
# Gruplar:  pos_en (Ingilizce "Hey Asuna")  pos_tr (Turkce aksan)
#           neg (temiz negatif)             amb (icinde gercekten "asuna" gecen)
set -uo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
OUT="$ROOT/audio"
rm -rf "$OUT"
mkdir -p "$OUT/pos_en" "$OUT/pos_tr" "$OUT/neg" "$OUT/amb"

n=0
gen() { # gen <grup> <voice> <rate|-> <metin>
  local group="$1" voice="$2" rate="$3" text="$4"
  n=$((n + 1))
  local slug
  slug=$(printf '%s' "$voice" | tr -cd '[:alnum:]')
  local file
  file=$(printf '%s/%s/%03d_%s_r%s.wav' "$OUT" "$group" "$n" "$slug" "$rate")
  if [ "$rate" = "-" ]; then
    say -v "$voice" --file-format=WAVE --data-format=LEI16@16000 -o "$file" "$text"
  else
    say -v "$voice" -r "$rate" --file-format=WAVE --data-format=LEI16@16000 -o "$file" "$text"
  fi
}

WAKE="Hey Asuna"

# ---- pos_en: farkli aksan/cinsiyet, varsayilan hiz ----
for v in "Samantha" "Daniel" "Karen" "Moira" "Tessa" "Rishi" "Aman" "Tara" \
         "Eddy (English (US))" "Eddy (English (UK))" "Flo (English (US))" \
         "Flo (English (UK))" "Grandma (English (US))" "Grandpa (English (UK))" \
         "Reed (English (US))" "Rocko (English (UK))" "Sandy (English (US))" \
         "Shelley (English (UK))"; do
  gen pos_en "$v" - "$WAKE"
done

# ---- pos_en: hiz varyasyonlari (yavas / hizli) ----
gen pos_en "Samantha" 140 "$WAKE"
gen pos_en "Samantha" 200 "$WAKE"
gen pos_en "Samantha" 250 "$WAKE"
gen pos_en "Daniel" 150 "$WAKE"
gen pos_en "Daniel" 230 "$WAKE"
gen pos_en "Karen" 130 "$WAKE"
gen pos_en "Karen" 240 "$WAKE"
gen pos_en "Moira" 210 "$WAKE"
gen pos_en "Tessa" 145 "$WAKE"
gen pos_en "Rishi" 225 "$WAKE"
# ---- pos_en: cumle icinde / cevresinde konusma ----
gen pos_en "Samantha" - "Hey Asuna, are you there?"
gen pos_en "Daniel" - "So anyway, hey Asuna, open the project."

# ---- pos_tr: Turkce ses (kullanicinin gercek aksanina daha yakin) ----
gen pos_tr "Yelda" - "Hey Asuna"
gen pos_tr "Yelda" 150 "Hey Asuna"
gen pos_tr "Yelda" 220 "Hey Asuna"
gen pos_tr "Yelda" - "Hey Asuna, naber?"
gen pos_tr "Yelda" - "Hey Asunaa"
gen pos_tr "Yelda" 180 "Hey Asuna"

# ---- neg: rakip wake word'ler ----
gen neg "Samantha" - "Hey Alexa"
gen neg "Daniel" - "Hey Siri"
gen neg "Karen" - "Hey Google"
gen neg "Moira" - "OK Google, what is the weather today?"
gen neg "Tessa" - "Alexa, turn off the lights"
# ---- neg: fonetik yakin Ingilizce ----
gen neg "Samantha" - "As soon as possible, please"
gen neg "Daniel" - "It was unusual that day"
gen neg "Karen" - "He has a new laptop"
gen neg "Moira" - "A tuna sandwich, please"
gen neg "Rishi" - "The answer is unknown"
gen neg "Tessa" - "He was unaware of the issue"
gen neg "Samantha" - "Asuncion is the capital of Paraguay"
gen neg "Daniel" - "She asked Una to come over"
gen neg "Aman" - "The cat was under the table"
gen neg "Tara" - "Hey there, how are you doing?"
gen neg "Karen" - "Hey, can you hear me?"
# ---- neg: normal gelistirme konusmasi ----
gen neg "Samantha" - "I need to refactor this function before lunch"
gen neg "Daniel" - "Run the tests again and commit the changes"
gen neg "Moira" - "Please close the session and go idle"
# ---- neg: Turkce ----
gen neg "Yelda" - "Hesabina para yatirdim"
gen neg "Yelda" - "Asansore binelim mi"
gen neg "Yelda" - "Bugun hava cok guzel"
gen neg "Yelda" - "Yarin toplanti var mi"
gen neg "Yelda" - "Bu fonksiyonu bastan yazmam lazim"
gen neg "Yelda" - "Asli sunu anlatmak istiyorum"

# ---- amb: icinde gercekten "asuna" fonemleri gecen ifadeler ----
gen amb "Yelda" - "Asuna degil, Asli dedim"
gen amb "Yelda" - "Asuna bugun calismiyor"
gen amb "Samantha" - "Asuna is the name of the assistant"

echo "uretilen dosya sayisi: $n"
for g in pos_en pos_tr neg amb; do
  printf '%-8s %s\n' "$g" "$(ls "$OUT/$g" | wc -l | tr -d ' ')"
done
