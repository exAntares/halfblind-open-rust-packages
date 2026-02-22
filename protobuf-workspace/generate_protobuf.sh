#!/usr/bin/env bash

# Take the path of the location of the script, instead of whichever arbitrary path it was called from.
# This allows us to run the script from any location and still get the desired result (since we always have the script right above the protos)
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

export PATH="$HOME/.cargo/bin:$PATH"
mkdir -p "generated"
mkdir -p "generated/rust"
mkdir -p "generated/csharp"

# Ensure protoc-gen-prost version 0.4.0 is installed
PROTOC_GEN_PROST_VERSION="0.4.0"
if ! command -v protoc-gen-prost &> /dev/null || [[ "$(protoc-gen-prost --version 2>&1)" != "$PROTOC_GEN_PROST_VERSION" ]]; then
  echo "protoc-gen-prost $PROTOC_GEN_PROST_VERSION not found or version mismatch. Installing..."
  cargo install protoc-gen-prost --version "$PROTOC_GEN_PROST_VERSION"
fi

# Detect OS and set protoc path
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]] || [[ "$OSTYPE" == "win32" ]]; then
  PROTOC_PATH="protobuf-compiler/win/protoc.exe"
  PROTOC_RUST_PATH="$(where protoc-gen-prost | head -n 1 | sed 's/\\/\//g')"
elif [[ "$OSTYPE" == linux* ]]; then
  PROTOC_PATH="protobuf-compiler/linux_x64/protoc"
  PROTOC_RUST_PATH="$(which protoc-gen-prost)"
else
  PROTOC_PATH="protobuf-compiler/osx/protoc"
  PROTOC_RUST_PATH="$(which protoc-gen-prost)"
fi
echo "Using protoc at $PROTOC_PATH"

echo "Generating Rust code from .proto files..."

# Find all .proto files recursively
PROTO_FILES=$(find "protos" -name "*.proto" | sort -r )

echo "Compiling:\n ${PROTO_FILES}\n"

## GENERATE RUST
"$PROTOC_PATH" \
  --proto_path=protos \
  --plugin=protoc-gen-prost="$PROTOC_RUST_PATH" \
  --prost_out=generated/rust $PROTO_FILES
# When updating to protoc-gen-prost version 0.5.0+,
#  --prost_out=generated/rust \
#  --prost_opt=flat_output_dir=true $PROTO_FILES

cp -R generated/rust/protobuf_network.rs ../halfblind-protobuf-network/src/
cp -R generated/rust/protobuf_itemdefinition.rs ../halfblind-protobuf-itemdefinitions/src/
cp -R generated/rust/protobuf_game.rs ../proto-gen/src/

## GENERATE C#
echo "Generating C# code from .proto files..."

"$PROTOC_PATH" \
  --proto_path=protos \
  --csharp_out=generated/csharp \
  $PROTO_FILES

#cp -R generated/csharp/* ../unity-packages/halfblind-protobuf-network/Runtime/Generated

# CLEANUP local generated files
echo "Cleanup intermediate folder..."
rm -r generated

echo "Files generated"
