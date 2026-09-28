#!/bin/sh
set -eu

if [ "$(id -u)" -ne 0 ]; then
  echo "AirSlate Linux post-install must run as root" >&2
  exit 1
fi

/sbin/modprobe uinput
/usr/bin/udevadm control --reload-rules
/usr/bin/udevadm trigger --action=add --subsystem=misc --sysname-match=uinput
/usr/bin/udevadm settle
