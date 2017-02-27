# This script takes care of testing your crate

set -ex

eval $CRATES_CONFIG

echo "--- Check format ---"
for PKG in "${FMT_CHECK_CRATES[@]}"
do
	echo "-- checking: ${PKG}"
	
	cd $PKG
	# TODO: do we want to do that target specific?
    cargo fmt -- --write-mode=diff
    cd ..
done


echo "--- Test ffi_utils ---"
cd ffi_utils
cargo test --target $TARGET --verbose --release
cd ..


echo "--- Check compilation against actual routing ---"
for PKG in "${TEST_CRATES[@]}"
do
	echo "-- compiling: ${PKG}"
	cd $PKG
    cargo rustc --target $TARGET --verbose --release
    cargo rustc --target $TARGET --verbose --features testing --release -- --test -Zno-trans
    cd ..
done


echo "--- Test against mock ---"
for PKG in "${TEST_CRATES[@]}"
do
	echo "-- testing: ${PKG}"
	cd $PKG
	cargo test --target $TARGET --verbose --release --features "$Features"
	cd ..
done
