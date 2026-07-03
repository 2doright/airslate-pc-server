import { invoke } from '@tauri-apps/api/core';

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
  configVersion: number;
  launchAtStartup: boolean;
  ipv4Values: string[];
  pressureCurve: PressureCurveDto;
  monitors: MonitorDto[];
  activePresetId: string;
  presets: ShortcutPresetDto[];
  effectiveBindings: BindingDto[];
  sessionStatus: { hasActiveSession: boolean };
}

export async function getAppBootstrap() {
  return invoke<AppBootstrapDto>('get_app_bootstrap');
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

export async function setRadialOuterSlot(index: number, keys: string[]) {
  return invoke('set_radial_outer_slot', { payload: { index, keys } });
}

export async function setRadialInnerBindings(payload: RadialInnerBindingsPayload) {
  return invoke('set_radial_inner_bindings', { payload });
}

export async function setRadialInnerEnabled(enabled: boolean) {
  return invoke('set_radial_inner_enabled', { payload: { enabled } });
}
