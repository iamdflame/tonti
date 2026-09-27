#!/usr/bin/env bash
# Deploy Tonti to Robinhood Chain and wire it together.
#
#   deploy/deploy.sh <private-key-file> [ISO3,ISO3,...]
#
# The key file must be mode 600 and hold one hex private key. cargo-stylus reads it from the file;
# Foundry (cast/forge) only accepts a raw --private-key argument, which is visible to this user's
# other processes while a command runs. Use a fresh deployer wallet that holds only deploy funds.
# Countries default to all 12 fitted ones (~0.005 ETH at 0.027 gwei); pass a list to load fewer.
#
# Rehearsal on a local fork (anvil --fork-url <mainnet rpc>): anvil can't run Stylus, so pass
# ACTUARY=<addr> POOL=<addr> to skip only the two Stylus deploys, RPC=<anvil url>, and
# DEPLOYMENT_OUT=<file> to keep config/deployment.json untouched. Everything else runs for real.
set -euo pipefail

KEY_FILE=${1:?usage: deploy/deploy.sh <private-key-file> [ISO3,...]}
COUNTRIES=${2:-}
RPC=${RPC:-https://rpc.mainnet.chain.robinhood.com}
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CFG=$ROOT/config/robinhood-mainnet.json
RUNS=$(python3 -c "import sys; sys.path.insert(0, '$ROOT/actuarial'); from paths import RUNS; print(RUNS)")
OUT_DIR=${OUT_DIR:-$RUNS/artifacts}
DEPLOYMENT_OUT=${DEPLOYMENT_OUT:-$ROOT/config/deployment.json}
mkdir -p "$OUT_DIR"

[ "$(stat -c %a "$KEY_FILE")" = "600" ] || { echo "key file must be chmod 600"; exit 1; }
PK=$(tr -d ' \n\r' < "$KEY_FILE")
DEPLOYER=$(cast wallet address --private-key "$PK")
CHAIN=$(cast chain-id --rpc-url "$RPC")
BAL=$(cast balance "$DEPLOYER" --rpc-url "$RPC")
echo "deployer $DEPLOYER on chain $CHAIN, balance $(cast to-unit "$BAL" ether) ETH"
[ "$CHAIN" = "4663" ] || [ "$CHAIN" = "46630" ] || { echo "not Robinhood Chain"; exit 1; }

j() { python3 -c "import json,sys; d=json.load(open('$CFG')); print($1)"; }
USDG=$(j "d['tokens']['USDG']['address']")
STEAK=$(j "d['morpho']['steakhouseUSDG']['address']")
SGOV=$(j "d['tokens']['SGOV']['address']")
SPY=$(j "d['tokens']['SPY']['address']")
SGOV_FEED=$(j "d['chainlink']['SGOV_USD']['address']")
SPY_FEED=$(j "d['chainlink']['SPY_USD']['address']")
PM=$(j "d['uniswapV4']['poolManager']")

send() { cast send "$@" --rpc-url "$RPC" --private-key "$PK" --json | python3 -c "import json,sys; r=json.load(sys.stdin); print(r['transactionHash'], 'gas', int(r['gasUsed'],16))"; }

stylus_deploy() { # $1 = crate dir; prints the deployed address
  (cd "$ROOT/engine/$1" && . ../.cargo-env && cargo stylus deploy --endpoint "$RPC" --private-key-path "$KEY_FILE" --no-verify 2>&1) \
    | tee -a "$OUT_DIR/deploy.log" | sed 's/\x1b\[[0-9;]*m//g' | grep -oiE 'deployed code at address:? *0x[0-9a-f]{40}' | grep -oiE '0x[0-9a-f]{40}' | tail -1
}

if [ -n "${ACTUARY:-}" ] && [ -n "${POOL:-}" ]; then
  echo "== Stylus: using ACTUARY=$ACTUARY POOL=$POOL (rehearsal)"
else
  echo "== Stylus: Actuary"; ACTUARY=$(stylus_deploy actuary-stylus); echo "Actuary $ACTUARY"
  echo "== Stylus: TontiPool"; POOL=$(stylus_deploy pool-stylus); echo "TontiPool $POOL"
fi
[ -n "$ACTUARY" ] && [ -n "$POOL" ] || { echo "Stylus deploy failed; see $OUT_DIR/deploy.log"; exit 1; }

echo "== Solidity: Treasury, LifeRegistry"
cd "$ROOT/contracts"
TREASURY=$(forge create src/Treasury.sol:Treasury --rpc-url "$RPC" --private-key "$PK" --broadcast --json \
  --constructor-args "$USDG" "$STEAK" "$SGOV" "$SPY" "$SGOV_FEED" "$SPY_FEED" "$PM" \
  "($USDG,$SGOV,375,4,0x0000000000000000000000000000000000000000)" \
  "($SPY,$USDG,500,5,0x0000000000000000000000000000000000000000)" | python3 -c "import json,sys; print(json.load(sys.stdin)['deployedTo'])")
REGISTRY=$(forge create src/LifeRegistry.sol:LifeRegistry --rpc-url "$RPC" --private-key "$PK" --broadcast --json \
  --constructor-args "$USDG" | python3 -c "import json,sys; print(json.load(sys.stdin)['deployedTo'])")
echo "Treasury $TREASURY  LifeRegistry $REGISTRY"

# Identity: until a trust-minimised adapter (ZKPassport, national-ID QR) is live, an attester key
# signs EIP-712 identity statements after a document check. ATTESTER defaults to the deployer.
ATTESTER=${ATTESTER:-$DEPLOYER}
IDENTITY=$(forge create src/AttestedIdentity.sol:AttestedIdentity --rpc-url "$RPC" --private-key "$PK" --broadcast --json \
  --constructor-args "$ATTESTER" "$REGISTRY" | python3 -c "import json,sys; print(json.load(sys.stdin)['deployedTo'])")
echo "AttestedIdentity $IDENTITY (attester $ATTESTER)"

echo "== Wiring"
send "$ACTUARY" "init()"
send "$POOL" "init(address,address,address,address)" "$TREASURY" "$REGISTRY" "$ACTUARY" "$USDG"
send "$TREASURY" "setPool(address)" "$POOL"
send "$REGISTRY" "setPool(address)" "$POOL"
send "$REGISTRY" "setVerifier(address,bool)" "$IDENTITY" true

echo "== Actuary data (mortality cohorts + market assumptions)"
python3 "$ROOT/deploy/actuary_batches.py" "$COUNTRIES" | while read -r a b c d e; do
  if [ "$a" = "MARKET" ]; then
    send "$ACTUARY" "setMarket(int256,int256,int256,int256)" "$b" "$c" "$d" "$e"
  else
    send "$ACTUARY" "setMortalityBatch(uint256[],int256[],int256[],int256[],int256[])" "$a" "$b" "$c" "$d" "$e"
  fi
done

echo "== Governance: a 48-hour timelock owns every contract; the deployer may only pause"
cd "$ROOT/contracts"
TIMELOCK=$(forge create lib/openzeppelin-contracts/contracts/governance/TimelockController.sol:TimelockController \
  --rpc-url "$RPC" --private-key "$PK" --broadcast --json \
  --constructor-args 172800 "[$DEPLOYER]" "[$DEPLOYER]" 0x0000000000000000000000000000000000000000 \
  | python3 -c "import json,sys; print(json.load(sys.stdin)['deployedTo'])")
send "$POOL" "setGuardian(address)" "$DEPLOYER"
for C in "$ACTUARY" "$POOL" "$TREASURY" "$REGISTRY"; do
  send "$C" "transferOwnership(address)" "$TIMELOCK"
done
echo "Timelock $TIMELOCK"

STAMP=$(date -u +%Y%m%dT%H%M%SZ)
cat > "$OUT_DIR/deployment-$STAMP.json" <<JSON
{ "chainId": $CHAIN, "deployer": "$DEPLOYER", "actuary": "$ACTUARY", "pool": "$POOL",
  "treasury": "$TREASURY", "lifeRegistry": "$REGISTRY", "timelock": "$TIMELOCK", "attestedIdentity": "$IDENTITY", "attester": "$ATTESTER", "usdg": "$USDG",
  "countries": "${COUNTRIES:-all}" }
JSON
cp "$OUT_DIR/deployment-$STAMP.json" "$DEPLOYMENT_OUT"
echo "deployed -> $DEPLOYMENT_OUT"
