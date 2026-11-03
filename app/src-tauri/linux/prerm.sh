#!/bin/sh
# Remove the TUN service (if it was ever installed) together with the package.
set -e
if [ "$1" = "remove" ] || [ "$1" = "purge" ]; then
  if [ -x /opt/choclite/service/clash-rs-desktop-service ]; then
    /opt/choclite/service/clash-rs-desktop-service --uninstall || true
  fi
fi
exit 0
