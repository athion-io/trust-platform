#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="${ROOT_DIR}/target/browser-analysis-wasm"
PKG_DIR="${OUT_DIR}/pkg"
WEB_SRC="${ROOT_DIR}/docs/internal/prototypes/browser_analysis_wasm_spike/web"
WEB_OUT="${OUT_DIR}/web"
DEMO_DIR="${ROOT_DIR}/docs/demo"
RUNTIME_UI_DIR="${ROOT_DIR}/crates/trust-runtime/src/web/ui"
RUNTIME_ASSETS_DIR="${RUNTIME_UI_DIR}/assets"
RUNTIME_BASE_CSS="${RUNTIME_UI_DIR}/chunks/base-css/base-01.css"

MODE="demo"
USE_WASM_OPT=1
WASM_OPT_PASSES="-Oz"

usage() {
  cat <<'EOF'
Usage: scripts/build_wasm.sh [options]

Isolated wasm/demo build with wasm-focused profile tuning.
This does not modify existing build scripts.

Options:
  --mode <demo|pkg>       Build demo assets (default) or package only.
  --no-wasm-opt           Skip wasm-opt even if installed.
  --wasm-opt-passes <p>   Passes for wasm-opt (default: -Oz).
  -h, --help              Show this help.

Environment overrides (optional):
  WASM_PROFILE_OPT_LEVEL     default: z
  WASM_PROFILE_LTO           default: fat
  WASM_PROFILE_CODEGEN_UNITS default: 1
  WASM_PROFILE_PANIC         default: abort
  WASM_PROFILE_STRIP         default: debuginfo
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --mode)
      MODE="${2:-}"
      shift 2
      ;;
    --no-wasm-opt)
      USE_WASM_OPT=0
      shift
      ;;
    --wasm-opt-passes)
      WASM_OPT_PASSES="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

if [[ "${MODE}" != "demo" && "${MODE}" != "pkg" ]]; then
  echo "error: --mode must be one of: demo, pkg" >&2
  exit 2
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "error: wasm-pack is required." >&2
  echo "install: cargo install wasm-pack" >&2
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown >/dev/null
fi

export CARGO_PROFILE_RELEASE_OPT_LEVEL="${WASM_PROFILE_OPT_LEVEL:-z}"
export CARGO_PROFILE_RELEASE_LTO="${WASM_PROFILE_LTO:-fat}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${WASM_PROFILE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_PANIC="${WASM_PROFILE_PANIC:-abort}"
export CARGO_PROFILE_RELEASE_STRIP="${WASM_PROFILE_STRIP:-debuginfo}"

REMAP_FLAGS="--remap-path-prefix=${ROOT_DIR}=."
if [[ -n "${HOME:-}" ]]; then
  REMAP_FLAGS+=" --remap-path-prefix=${HOME}=~"
fi
export RUSTFLAGS="${RUSTFLAGS:-} ${REMAP_FLAGS}"

rm -rf "${PKG_DIR}" "${WEB_OUT}"
mkdir -p "${PKG_DIR}" "${WEB_OUT}"

echo "==> Building trust-wasm-analysis with wasm size profile..."
(
  cd "${ROOT_DIR}/crates/trust-wasm-analysis"
  wasm-pack build \
    --target web \
    --out-dir "${PKG_DIR}" \
    --out-name trust_wasm_analysis \
    -- \
    --features wasm
)

WASM_FILE="${PKG_DIR}/trust_wasm_analysis_bg.wasm"
if [[ -f "${WASM_FILE}" && "${USE_WASM_OPT}" -eq 1 ]]; then
  if command -v wasm-opt >/dev/null 2>&1; then
    echo "==> Running wasm-opt ${WASM_OPT_PASSES}..."
    wasm-opt "${WASM_OPT_PASSES}" "${WASM_FILE}" -o "${WASM_FILE}.opt"
    mv "${WASM_FILE}.opt" "${WASM_FILE}"
  else
    echo "warn: wasm-opt not found (install with: brew install binaryen). Skipping extra wasm optimization."
  fi
fi

if [[ "${MODE}" == "demo" ]]; then
  echo "==> Copying demo assets..."
  mkdir -p "${DEMO_DIR}/wasm" "${DEMO_DIR}/assets"
  touch "${DEMO_DIR}/.nojekyll"

  cp "${PKG_DIR}/trust_wasm_analysis.js" "${DEMO_DIR}/wasm/"
  cp "${PKG_DIR}/trust_wasm_analysis_bg.wasm" "${DEMO_DIR}/wasm/"
  cp "${RUNTIME_UI_DIR}/wasm/analysis-client.js" "${DEMO_DIR}/wasm/"

  latest_monaco="$(ls -1t "${RUNTIME_UI_DIR}"/assets/ide-monaco.*.js 2>/dev/null | head -n 1 || true)"
  if [[ -n "${latest_monaco}" ]]; then
    cp "${latest_monaco}" "${DEMO_DIR}/assets/"
  else
    echo "warn: no ide-monaco.*.js asset found under ${RUNTIME_UI_DIR}/assets"
  fi

  cp -R "${WEB_SRC}/." "${WEB_OUT}/"
  cp "${RUNTIME_ASSETS_DIR}/favicon.svg" "${WEB_OUT}/favicon.svg"
  cp "${RUNTIME_ASSETS_DIR}/logo.svg" "${WEB_OUT}/logo.svg"

  {
    cat "${RUNTIME_BASE_CSS}"
    for css in "${RUNTIME_UI_DIR}"/chunks/ide-css/ide-*.css; do
      cat "${css}"
    done
  } > "${WEB_OUT}/runtime-styles.css"
fi

print_size() {
  local file="$1"
  if [[ -f "${file}" ]]; then
    local raw
    local gz
    raw="$(wc -c < "${file}" | tr -d ' ')"
    gz="$(gzip -9 -c "${file}" | wc -c | tr -d ' ')"
    echo "  - ${file#${ROOT_DIR}/}: raw=${raw} bytes, gzip=${gz} bytes"
  fi
}

echo ""
echo "==> Size report"
print_size "${PKG_DIR}/trust_wasm_analysis_bg.wasm"
print_size "${PKG_DIR}/trust_wasm_analysis.js"
if [[ "${MODE}" == "demo" ]]; then
  print_size "${DEMO_DIR}/wasm/trust_wasm_analysis_bg.wasm"
  print_size "${DEMO_DIR}/wasm/trust_wasm_analysis.js"
  latest_demo_monaco="$(ls -1t "${DEMO_DIR}"/assets/ide-monaco.*.js 2>/dev/null | head -n 1 || true)"
  if [[ -n "${latest_demo_monaco}" ]]; then
    print_size "${latest_demo_monaco}"
  fi
fi

echo ""
echo "Build complete (mode=${MODE})."
if [[ "${MODE}" == "demo" ]]; then
  echo "Serve demo: python3 -m http.server 8000 -d ${DEMO_DIR}"
fi
