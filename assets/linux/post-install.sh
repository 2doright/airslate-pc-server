#!/bin/sh
set -eu

# Ensure /dev/uinput exists when the kernel exposes uinput as a module.
if command -v modprobe >/dev/null 2>&1; then
  modprobe uinput >/dev/null 2>&1 || true
fi

# Reload the packaged rule and immediately re-apply it to an existing uinput node.
if command -v udevadm >/dev/null 2>&1; then
  udevadm control --reload-rules >/dev/null 2>&1 || true
  if [ -e /sys/class/misc/uinput ]; then
    udevadm trigger --action=add --subsystem-match=misc --sysname-match=uinput >/dev/null 2>&1 || true
  fi
fi

exit 0
