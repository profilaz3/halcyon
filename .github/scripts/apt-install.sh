#!/usr/bin/env bash
# Install the packages listed in $1, keeping downloaded .debs and package
# lists in $2 so a cached run installs without the network. When the cache
# is missing or stale, fall back to the mirrors with retries and short
# timeouts, so an archive outage fails in minutes instead of hanging.
set -euo pipefail
list=$1
cache=$2
mkdir -p "$cache/archives/partial" "$cache/lists/partial"
read -ra packages <<< "$(tr '\n' ' ' < "$list")"
apt=(sudo apt-get
  -o Acquire::Retries=3
  -o Acquire::http::Timeout=20
  -o Acquire::https::Timeout=20
  -o APT::Keep-Downloaded-Packages=true
  -o Dir::Cache::Archives="$cache/archives"
  -o Dir::State::Lists="$cache/lists")
if [ -n "$(ls -A "$cache/lists" 2>/dev/null | grep -v '^partial$\|^lock$')" ] &&
   "${apt[@]}" install -y --no-install-recommends --no-download "${packages[@]}"; then
  echo "Installed from the apt cache."
else
  echo "Cache missing or incomplete; downloading."
  "${apt[@]}" update
  "${apt[@]}" install -y --no-install-recommends "${packages[@]}"
fi
sudo rm -f "$cache/archives/lock" "$cache/lists/lock"
sudo chown -R "$(id -u):$(id -g)" "$cache"
