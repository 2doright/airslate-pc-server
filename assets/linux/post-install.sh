#!/bin/sh
set -eu

modprobe uinput
udevadm control --reload-rules
udevadm trigger --action=add --subsystem-match=misc --sysname-match=uinput
udevadm settle
