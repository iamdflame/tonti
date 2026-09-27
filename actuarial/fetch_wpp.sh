#!/usr/bin/env bash
# Download the UN WPP 2024 single-age life tables (CC BY 3.0 IGO) to the UNIQ drive.
# Resumable (curl -C -), retried, no time cap, three files in parallel; writes SHA256SUMS at the end.
set -u
D=$(python3 -c "import sys; sys.path.insert(0, '$(dirname "$0")'); from paths import DATA; print(DATA / 'wpp2024')")
B='https://population.un.org/wpp/assets/Excel%20Files/1_Indicator%20(Standard)/CSV_FILES'
FILES=(
  WPP2024_Life_Table_Complete_Medium_Female_1950-2023.csv.gz
  WPP2024_Life_Table_Complete_Medium_Male_1950-2023.csv.gz
  WPP2024_Life_Table_Complete_Medium_Female_2024-2100.csv.gz
  WPP2024_Life_Table_Complete_Medium_Male_2024-2100.csv.gz
)
mkdir -p "$D" && cd "$D" || exit 1

fetch() {
  local f=$1
  [ -f "$f" ] && { echo "have $f"; return 0; }
  for attempt in 1 2 3 4 5 6 7 8; do
    if curl -sS -A 'Mozilla/5.0' --retry 5 --retry-delay 10 -C - -o "$f.part" "$B/$f"; then
      local want; want=$(curl -sI -A 'Mozilla/5.0' "$B/$f" | tr -d '\r' | awk 'tolower($1)=="content-length:"{print $2}')
      if [ "$(stat -c %s "$f.part")" = "$want" ]; then mv "$f.part" "$f"; echo "ok $f $want"; return 0; fi
    fi
    echo "retry $attempt $f"
  done
  echo "FAILED $f"; return 1
}

for f in "${FILES[@]}"; do fetch "$f" & done
wait
sha256sum "${FILES[@]}" > SHA256SUMS && cat SHA256SUMS
