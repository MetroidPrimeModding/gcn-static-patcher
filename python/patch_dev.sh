#!/usr/bin/env bash -xe
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"

cd "${SCRIPT_DIR}"
PYTHONPATH="${SCRIPT_DIR}" python3 main.py iso --skip-hash -i prime2-orig.iso -o metroid-prime2-practice-mod.iso -m ./prime-practice
