#!/bin/sh
# Refreshes apt lists on hosted Ubuntu runners. The azure mirror can stall, so
# after a short try the sources move to archive.ubuntu.com and apt tries again.
set -eu
if sudo timeout 120 apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20 update; then
  exit 0
fi
echo "azure.archive.ubuntu.com did not answer; using archive.ubuntu.com"
for source in /etc/apt/sources.list.d/ubuntu.sources /etc/apt/sources.list /etc/apt/apt-mirrors.txt; do
  if [ -f "$source" ]; then
    sudo sed -i 's|http://azure.archive.ubuntu.com/ubuntu|http://archive.ubuntu.com/ubuntu|g' "$source"
  fi
done
sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 update
