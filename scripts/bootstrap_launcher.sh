#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../launcher"
flutter pub get
flutter create --platforms=linux,windows --project-name backrooms_infinity_launcher .
