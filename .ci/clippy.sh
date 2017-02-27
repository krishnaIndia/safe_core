#/!bin/bash
set -x

cd ffi_utils
unset Features
cargo clippy
cargo clippy --profile=test
cd ..

export Features="use-mock-routing testing"

echo "--- Clippy run ---"
for PKG in "${TEST_CRATES[@]}"
	echo "-- clippy on: ${PKG}"

	cd $PKG
	cargo clippy
	cargo clippy --profile=test --features="$Features"
	cd ..
done