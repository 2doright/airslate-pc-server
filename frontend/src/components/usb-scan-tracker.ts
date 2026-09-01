import type { UsbScanDevice } from '../lib/tauri';

export class UsbScanTracker {
  private baselineKeys: Set<string> | null = null;
  private readonly observations = new Map<string, UsbScanDevice>();

  observe(snapshot: UsbScanDevice[]) {
    const hdcDevices = snapshot.filter(isHdcDevice);
    const currentKeys = new Set(hdcDevices.map(usbPhysicalKey));

    if (this.baselineKeys === null) {
      this.baselineKeys = currentKeys;
      return this.currentObservations();
    }

    for (const key of this.baselineKeys) {
      if (!currentKeys.has(key)) this.baselineKeys.delete(key);
    }

    for (const device of hdcDevices) {
      const key = usbPhysicalKey(device);
      if (this.baselineKeys.has(key)) continue;

      const observation = this.observations.get(key);
      this.observations.set(
        key,
        observation === undefined
          ? captureInitialDescriptor(device)
          : retainInitialDescriptor(observation, device),
      );
    }

    for (const key of this.observations.keys()) {
      if (!currentKeys.has(key)) this.observations.delete(key);
    }

    return this.currentObservations();
  }

  private currentObservations() {
    return [...this.observations.values()];
  }
}

function captureInitialDescriptor(device: UsbScanDevice): UsbScanDevice {
  return {
    ...device,
    initialManufacturer: device.initialManufacturer ?? device.manufacturer,
    initialProduct: device.initialProduct ?? device.product,
    initialInterfaces: (device.initialInterfaces ?? device.interfaces).map((usbInterface) => ({
      ...usbInterface,
    })),
  };
}

function retainInitialDescriptor(
  observation: UsbScanDevice,
  current: UsbScanDevice,
): UsbScanDevice {
  return {
    ...current,
    initialManufacturer: observation.initialManufacturer,
    initialProduct: observation.initialProduct,
    initialInterfaces: observation.initialInterfaces,
  };
}

function isHdcDevice(device: UsbScanDevice) {
  return [device.product, device.initialProduct]
    .some((name) => name?.trim().toLowerCase() === 'hdc device');
}

export function usbPhysicalKey(device: UsbScanDevice) {
  return `${device.busId}\u0000${device.portChain.join('.')}`;
}
