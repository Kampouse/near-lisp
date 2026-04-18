# near-lisp — Build, Test, Deploy
#
# Usage:
#   make deploy         Build + deploy to testnet (self-contained, installs what's missing)
#   make build          Build WASM only
#   make test           Run unit tests
#   make test-sandbox   Run sandbox tests (real WASM on local sandbox)
#   make testnet        Run testnet integration tests (needs env vars)
#   make call CODE=...  Eval Lisp on-chain
#   make repl           Local REPL
#   make clean          Clean build artifacts

ACCOUNT   := kampy.testnet
NETWORK   := testnet
TOOLCHAIN := 1.86.0
CREDENTIAL := $(HOME)/.near-credentials/$(NETWORK)/$(ACCOUNT).json

# ── Self-healing deploy ─────────────────────────────────────

.PHONY: _ensure-cargo-near _ensure-toolchain _ensure-target _ensure-creds deploy

_ensure-cargo-near:
	@which cargo-near > /dev/null 2>&1 || (echo "Installing cargo-near..." && cargo install cargo-near)

_ensure-toolchain:
	@rustup toolchain list | grep -q '$(TOOLCHAIN)' || (echo "Installing Rust $(TOOLCHAIN)..." && rustup toolchain install $(TOOLCHAIN))

_ensure-target: _ensure-toolchain
	@rustup target list --toolchain $(TOOLCHAIN) --installed | grep -q 'wasm32-unknown-unknown' || (echo "Adding wasm32 target for $(TOOLCHAIN)..." && rustup target add wasm32-unknown-unknown --toolchain $(TOOLCHAIN))

_ensure-creds:
	@test -f $(CREDENTIAL) || (echo "ERROR: Missing credentials at $(CREDENTIAL)" && echo "Run: near account import-account using-private-key <KEY> kampy.testnet network-config testnet" && exit 1)

deploy: _ensure-cargo-near _ensure-target _ensure-creds
	@echo "Deploying $(ACCOUNT) to $(NETWORK)..."
	cargo near deploy build-non-reproducible-wasm --no-abi --override-toolchain $(TOOLCHAIN) \
		$(ACCOUNT) without-init-call network-config $(NETWORK) sign-with-legacy-keychain send

# ── Build ────────────────────────────────────────────────────

build: _ensure-cargo-near _ensure-target
	cargo near build non-reproducible-wasm --no-abi --override-toolchain $(TOOLCHAIN)

# ── Test ─────────────────────────────────────────────────────

.PHONY: test test-unit test-sandbox test-examples test-fuzz bench testnet check fmt clippy

test: test-unit
	@echo "All tests passed."

test-unit:
	cargo test --lib

test-sandbox: build
	cargo test --test lisp_sandbox -- --nocapture

test-examples:
	cargo test --test test_examples

test-fuzz:
	cargo test --test fuzz_test

bench:
	cargo test --test bench_gas --test bench_max_loop -- --nocapture

testnet: build
	cargo test --test lisp_testnet -- --nocapture

# ── On-chain calls ──────────────────────────────────────────

.PHONY: call call-script view view-policies view-scripts view-modules view-whitelist view-gas balance

call:
	@echo "Evaluating: $(CODE)"
	near contract call-function as-transaction $(ACCOUNT) eval \
		json-args '{"code": "$(CODE)"}' \
		prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
		sign-as $(ACCOUNT) network-config $(NETWORK) sign-with-legacy-keychain send

call-script:
	near contract call-function as-transaction $(ACCOUNT) eval_script_with_input \
		json-args '{"name": "$(SCRIPT)", "input_json": "$(INPUT)"}' \
		prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
		sign-as $(ACCOUNT) network-config $(NETWORK) sign-with-legacy-keychain send

view:
	near contract call-function as-read-only $(ACCOUNT) get_owner json-args '{}' network-config $(NETWORK) now

view-policies:
	near contract call-function as-read-only $(ACCOUNT) list_policies json-args '{}' network-config $(NETWORK) now

view-scripts:
	near contract call-function as-read-only $(ACCOUNT) list_scripts json-args '{}' network-config $(NETWORK) now

view-modules:
	near contract call-function as-read-only $(ACCOUNT) list_modules json-args '{}' network-config $(NETWORK) now

view-whitelist:
	near contract call-function as-read-only $(ACCOUNT) get_eval_whitelist json-args '{}' network-config $(NETWORK) now

view-gas:
	near contract call-function as-read-only $(ACCOUNT) get_gas_limit json-args '{}' network-config $(NETWORK) now

# ── Account ─────────────────────────────────────────────────

.PHONY: balance repl clean

balance:
	near account view-account-summary $(ACCOUNT) network-config $(NETWORK) now

# ── Local ────────────────────────────────────────────────────

repl:
	cargo run --bin repl

# ── Clean ────────────────────────────────────────────────────

clean:
	cargo clean
	@echo "Cleaned."

check:
	cargo check

fmt:
	cargo fmt

clippy:
	cargo clippy -- -W warnings
