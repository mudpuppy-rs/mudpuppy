#
# A crude script to symlink the python-examples into the
# standard Mudpuppy config directory.
#
#!/usr/bin/env bash

set -e

SCRIPT_PATH=$(dirname $(realpath -s $0))

DEST_DIR="$HOME/.config/mudpuppy"
mkdir -p "$DEST_DIR"

for file in *.py
do
    FILENAME="$(basename "$file")"
    ln --symbolic --force --verbose "$SCRIPT_PATH/$file" "$DEST_DIR/$FILENAME"
done
