import { invoke } from '@tauri-apps/api/core';

export type HoverMovePolicyLevel = 0 | 1 | 2 | 3;

export interface ShortcutPresetDto {
  id: string;
  name: string;
  active: boolean;
}

export type RadialInnerSlotId = 'top' | 'right' | 'bottom' | 'left';

export interface RadialInnerSlotDto {
  slot: RadialInnerSlotId;
  label: string;
  key: string;
}

export interface RadialOuterSlotDto {
  index: number;
  label: string;
  angleLabel: string;
  keys: string[];
}

export type RadialInnerBindingsPayload = Record<RadialInnerSlotId, string>;

export type ActionDto =
  | { kind: 'holdKey'; keys: string[] }
  | { kind: 'triggerChord'; keys: string[] }
  | { kind: 'advanced'; label: string; detail: string; radialInnerEnabled: boolean; radialInnerSlots: RadialInnerSlotDto[]; radialOuterSlots: RadialOuterSlotDto[] };

export interface MonitorDto {
  id: string;
  deviceName: string;
  label: string;
  isPrimary: boolean;
  pixelWidth: number;
  pixelHeight: number;
  selected: boolean;
}

export interface BindingDto {
  id: string;
  label: string;
  category: string;
  presetAction: ActionDto;
  currentAction: ActionDto;
  usesPreset: boolean;
  editableKeys: string[] | null;
  specialActions: { id: string; label: string }[];
  activeSpecialAction: string;
}

export interface PressureControlPointDto {
  x: number;
  y: number;
}

export interface PressureCurveDto {
  controlPoint1: PressureControlPointDto;
  controlPoint2: PressureControlPointDto;
}

export interface AppBootstrapDto {
  distribution: 'installed' | 'portable';
  configVersion: number;
  launchAtStartup: boolean;
  showLaunchAtStartupOnMainPage: boolean;
  latestContactMoveOnly: boolean;
  latestContactMoveToleranceMs: number;
  hoverMovePolicy: HoverMovePolicyLevel;
  preemptPreviousStroke: boolean;
  preciseAnchorCorrectionEnabled: boolean;
  usbInterface: string;
  ipv4Values: string[];
  pressureCurve: PressureCurveDto;
  monitors: MonitorDto[];
  activePresetId: string;
  presets: ShortcutPresetDto[];
  effectiveBindings: BindingDto[];
  sessionStatus: { hasActiveSession: boolean };
  usbStatus: UsbStatusEvent;
}

export interface SessionStatusEvent {
  hasActiveSession: boolean;
}

export interface UsbStatusEvent {
  state: 'waiting' | 'waiting_accessory' | 'authorizing' | 'handshaking' | 'connected' | 'error';
  detail: string;
  retryable: boolean;
  device: UsbDeviceInfo | null;
}

export interface UsbDeviceInfo {
  vendorId: number;
  productId: number;
  busId: string;
  portChain: number[];
  configuration: number | null;
  interfaceNumber: number | null;
  alternateSetting: number | null;
  bulkInEndpoint: number | null;
  bulkOutEndpoint: number | null;
  bulkInMaxPacketSize: number | null;
  bulkOutMaxPacketSize: number | null;
}

export interface UsbScanDevice {
  vendorId: number;
  productId: number;
  busId: string;
  portChain: number[];
  manufacturer: string | null;
  product: string | null;
  interfaces: UsbScanInterface[];
  initialManufacturer: string | null;
  initialProduct: string | null;
  initialInterfaces: UsbScanInterface[] | null;
}

export interface UsbScanInterface {
  interfaceNumber: number;
  classCode: number;
  subclass: number;
  protocol: number;
}

export async function getAppBootstrap() {
  return invoke<AppBootstrapDto>('get_app_bootstrap');
}

export async function disconnectActiveSession() {
  return invoke<SessionStatusEvent>('disconnect_active_session');
}

export async function retryUsbConnection() {
  return invoke('retry_usb_connection');
}

export async function scanUsbDevices() {
  return invoke<UsbScanDevice[]>('scan_usb_devices');
}

export async function getLanIpv4Values() {
  return invoke<string[]>('get_lan_ipv4_values');
}

export async function openExternal(url: string) {
  return invoke('open_external', { url });
}

export async function setSelectedMonitor(monitorId: string) {
  return invoke('set_selected_monitor', { monitorId });
}

export async function setPressureCurve(curve: PressureCurveDto) {
  return invoke('set_pressure_curve', { payload: curve });
}

export async function setLaunchAtStartup(enabled: boolean) {
  return invoke('set_launch_at_startup', { enabled });
}

export async function setShowLaunchAtStartupOnMainPage(enabled: boolean) {
  return invoke('set_show_launch_at_startup_on_main_page', { enabled });
}

export async function setLatestContactMoveOnly(enabled: boolean) {
  return invoke('set_latest_contact_move_only', { enabled });
}

export async function setLatestContactMoveToleranceMs(toleranceMs: number) {
  return invoke('set_latest_contact_move_tolerance_ms', { toleranceMs });
}

export async function setHoverMovePolicy(level: HoverMovePolicyLevel) {
  return invoke('set_hover_move_policy', { level });
}

export async function setPreemptPreviousStroke(enabled: boolean) {
  return invoke('set_preempt_previous_stroke', { enabled });
}

export async function setPreciseAnchorCorrectionEnabled(enabled: boolean) {
  return invoke('set_precise_anchor_correction_enabled', { enabled });
}

export async function setUsbInterface(interfaceValue: string) {
  return invoke('set_usb_interface', { interface: interfaceValue });
}

export async function selectShortcutPreset(presetId: string) {
  return invoke('select_shortcut_preset', { payload: { presetId } });
}

export async function createShortcutPreset(name: string) {
  return invoke('create_shortcut_preset', { payload: { name } });
}

export async function renameShortcutPreset(presetId: string, name: string) {
  return invoke('rename_shortcut_preset', { payload: { presetId, name } });
}

export async function deleteShortcutPreset(presetId: string) {
  return invoke('delete_shortcut_preset', { payload: { presetId } });
}

export async function resetShortcutPreset(presetId: string) {
  return invoke('reset_shortcut_preset', { payload: { presetId } });
}

export async function setBindingKeys(bindingId: string, keys: string[]) {
  return invoke('set_binding_keys', { payload: { bindingId, keys } });
}

export async function setBindingSpecialAction(bindingId: string, specialAction: string) {
  return invoke('set_binding_special_action', { payload: { bindingId, specialAction } });
}

export async function setRadialOuterSlot(index: number, keys: string[]) {
  return invoke('set_radial_outer_slot', { payload: { index, keys } });
}

export async function setRadialInnerBindings(payload: RadialInnerBindingsPayload) {
  return invoke('set_radial_inner_bindings', { payload });
}

export async function setRadialInnerEnabled(enabled: boolean) {
  return invoke('set_radial_inner_enabled', { payload: { enabled } });
}
