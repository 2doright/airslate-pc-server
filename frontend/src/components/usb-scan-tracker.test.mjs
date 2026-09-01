import assert from 'node:assert/strict';
import test from 'node:test';

import { UsbScanTracker } from './usb-scan-tracker.ts';

const DEFAULT_INTERFACE = usbInterface(0xFF, 0x50, 0x01);
const CUSTOM_INTERFACE = usbInterface(0xFF, 0x42, 0x01);
const REENUMERATED_INTERFACE = usbInterface(0xFF, 0xFF, 0x00);

test('shows a newly inserted HDC device before re-enumeration', () => {
  const tracker = new UsbScanTracker();

  assert.deepEqual(tracker.observe([]), []);
  const [observed] = tracker.observe([hdcDevice(CUSTOM_INTERFACE)]);

  assert.deepEqual(observed.initialInterfaces, [CUSTOM_INTERFACE]);
  assert.deepEqual(observed.interfaces, [CUSTOM_INTERFACE]);
});

test('retains the default initial signature when the same device re-enumerates', () => {
  const tracker = new UsbScanTracker();

  tracker.observe([]);
  const [initial] = tracker.observe([hdcDevice(DEFAULT_INTERFACE)]);
  const [reenumerated] = tracker.observe([hdcDevice(REENUMERATED_INTERFACE, {
    product: 'AirSlate',
    initialProduct: 'HDC Device',
    initialInterfaces: [DEFAULT_INTERFACE],
  })]);

  assert.deepEqual(initial.initialInterfaces, [DEFAULT_INTERFACE]);
  assert.deepEqual(reenumerated.initialInterfaces, [DEFAULT_INTERFACE]);
  assert.deepEqual(reenumerated.interfaces, [REENUMERATED_INTERFACE]);
});

test('removes an observed device when it is unplugged', () => {
  const tracker = new UsbScanTracker();

  tracker.observe([]);
  assert.equal(tracker.observe([hdcDevice(DEFAULT_INTERFACE)]).length, 1);
  assert.deepEqual(tracker.observe([]), []);
});

test('uses backend initial descriptors when the first UI snapshot is already re-enumerated', () => {
  const tracker = new UsbScanTracker();
  const device = hdcDevice(REENUMERATED_INTERFACE, {
    product: 'AirSlate',
    initialProduct: 'HDC Device',
    initialInterfaces: [CUSTOM_INTERFACE],
  });

  tracker.observe([]);
  const [observed] = tracker.observe([device]);

  assert.deepEqual(observed.initialInterfaces, [CUSTOM_INTERFACE]);
  assert.deepEqual(observed.interfaces, [REENUMERATED_INTERFACE]);
});

test('ignores devices already present when the scan starts until they are reinserted', () => {
  const tracker = new UsbScanTracker();
  const device = hdcDevice(CUSTOM_INTERFACE);

  assert.deepEqual(tracker.observe([device]), []);
  assert.deepEqual(tracker.observe([]), []);
  assert.equal(tracker.observe([device]).length, 1);
});

function hdcDevice(usbScanInterface, overrides = {}) {
  return {
    vendorId: 0x12D1,
    productId: 0x1101,
    busId: 'bus',
    portChain: [2],
    manufacturer: 'Huawei',
    product: 'HDC Device',
    interfaces: [usbScanInterface],
    initialManufacturer: null,
    initialProduct: null,
    initialInterfaces: null,
    ...overrides,
  };
}

function usbInterface(classCode, subclass, protocol) {
  return {
    interfaceNumber: 0,
    classCode,
    subclass,
    protocol,
  };
}
