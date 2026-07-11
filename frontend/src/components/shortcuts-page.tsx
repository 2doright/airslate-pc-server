import { useMemo, useState, type ReactNode } from 'react';
import type { ActionDto, AppBootstrapDto, BindingDto, RadialInnerBindingsPayload, RadialInnerSlotId } from '../lib/tauri';
import {
  createShortcutPreset,
  deleteShortcutPreset,
  resetShortcutPreset,
  selectShortcutPreset,
  setBindingKeys,
  setBindingSpecialAction,
  setRadialInnerBindings,
  setRadialInnerEnabled,
  setRadialOuterSlot,
} from '../lib/tauri';
import { RadialMenuVisual, orderRadialSlots, radialDirectionLabel } from './radial-menu-visual';
import { Badge, Button, KeyToken, Panel, PanelHeader, SelectField, Switch, TextInput } from './ui';

export type RecordingMode = 'single' | 'multi';

export type RecordingTarget =
  | { kind: 'binding'; bindingId: string; busyKey: string; mode: RecordingMode }
  | { kind: 'radialOuter'; index: number; busyKey: string; mode: 'multi' };

type RunAction = (key: string, action: () => Promise<unknown>) => Promise<void>;

const DEFAULT_PRESET_ID = 'default';

function PresetDeleteIcon() {
  return (
    <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M4 7h16" />
      <path d="M9 4h6" />
      <path d="M10 11v5" />
      <path d="M14 11v5" />
      <path d="M6 7l1 12h10l1-12" />
    </svg>
  );
}

export function ShortcutsPage(props: {
  data: AppBootstrapDto;
  busyKey: string | null;
  newPresetName: string;
  setNewPresetName: (value: string) => void;
  runAction: RunAction;
  recordingTarget: RecordingTarget | null;
  setRecordingTarget: (value: RecordingTarget | null) => void;
}) {
  const activePreset = props.data.presets.find((preset) => preset.active);
  const radialBinding = props.data.effectiveBindings.find((row) => row.id === 'gesture:two_pan');
  const sections = useBindingSections(props.data.effectiveBindings);

  return (
    <div className="shortcuts-layout">
      <Panel className="profile-panel">
        <PanelHeader title="快捷键预设" action={activePreset ? <Badge tone="success">{activePreset.name}</Badge> : null} />
        <div className="profile-grid">
          <label className="field-block">
            <span>当前预设</span>
            <SelectField
              value={props.data.activePresetId}
              disabled={props.busyKey === 'preset:select'}
              options={props.data.presets.map((preset) => ({
                value: preset.id,
                label: preset.name,
                action: {
                  ariaLabel: `删除预设 ${preset.name}`,
                  disabled: preset.id === DEFAULT_PRESET_ID || props.busyKey === `preset:delete:${preset.id}`,
                  icon: <PresetDeleteIcon />,
                  onClick: () => {
                    if (preset.id === DEFAULT_PRESET_ID) return;
                    void props.runAction(`preset:delete:${preset.id}`, () => deleteShortcutPreset(preset.id));
                  },
                },
              }))}
              onChange={(value) => {
                void props.runAction('preset:select', () => selectShortcutPreset(value));
              }}
            />
          </label>

          <label className="field-block profile-restore">
            <span aria-hidden="true" style={{ visibility: 'hidden' }}>占位</span>
            <Button
              type="button"
              tone="ghost"
              disabled={!activePreset || props.busyKey === `preset:reset:${activePreset?.id ?? ''}`}
              onClick={() => {
                if (!activePreset) return;
                void props.runAction(`preset:reset:${activePreset.id}`, () => resetShortcutPreset(activePreset.id));
              }}
            >
              恢复默认
            </Button>
          </label>

          <label className="field-block">
            <span>新建预设</span>
            <div className="inline-form">
              <TextInput value={props.newPresetName} onChange={(event) => props.setNewPresetName(event.target.value)} placeholder="输入名称" />
              <Button
                type="button"
                tone="primary"
                disabled={props.busyKey === 'preset:create'}
                onClick={() => {
                  const value = props.newPresetName.trim();
                  if (!value) return;
                  void props.runAction('preset:create', async () => {
                    await createShortcutPreset(value);
                    props.setNewPresetName('');
                  });
                }}
              >
                创建
              </Button>
            </div>
          </label>
        </div>
      </Panel>

      {radialBinding?.currentAction.kind === 'advanced' ? (
        <RadialMenuPanel
          action={radialBinding.currentAction}
          busyKey={props.busyKey}
          runAction={props.runAction}
          recordingTarget={props.recordingTarget}
          setRecordingTarget={props.setRecordingTarget}
        />
      ) : null}

      <div className="binding-section-stack">
        {sections.map((section) => (
          <Panel key={section.key} variant="tight" className={`binding-section-panel binding-section-panel--${section.key}`}>
            <PanelHeader title={section.title} />
            <BindingList
              rows={section.rows}
              busyKey={props.busyKey}
              runAction={props.runAction}
              recordingTarget={props.recordingTarget}
              setRecordingTarget={props.setRecordingTarget}
            />
          </Panel>
        ))}
      </div>
    </div>
  );
}

function RadialMenuPanel(props: {
  action: Extract<ActionDto, { kind: 'advanced' }>;
  busyKey: string | null;
  runAction: RunAction;
  recordingTarget: RecordingTarget | null;
  setRecordingTarget: (value: RecordingTarget | null) => void;
}) {
  const [activeSlot, setActiveSlot] = useState<number | null>(null);
  const orderedSlots = orderRadialSlots(props.action.radialOuterSlots);

  const innerEnabledBusy = props.busyKey === 'radial:inner-enabled';

  return (
    <Panel variant="hero" className="radial-panel">
      <PanelHeader
        title={<>
          径向菜单 <Badge tone="accent">双指平移</Badge> <Badge tone={props.action.radialInnerEnabled ? 'success' : 'warning'}>{props.action.radialInnerEnabled ? '内环已启用' : '内环已关闭'}</Badge>
        </>}
      />
      <div className="radial-editor-layout">
        <RadialMenuVisual
          action={props.action}
          innerEnabled={props.action.radialInnerEnabled}
          activeSlot={activeSlot}
          onActiveSlotChange={setActiveSlot}
          innerBusy={props.busyKey === 'radial:inner'}
          onInnerSwap={(from, to) => {
            const payload = swapRadialInnerSlots(props.action.radialInnerSlots, from, to);
            void props.runAction('radial:inner', () => setRadialInnerBindings(payload));
          }}
        />
        <div className="radial-slot-editor" aria-label="外环方向设置">
          {orderedSlots.map((slot) => {
            const editingBusyKey = `radial:outer:${slot.index}`;
            const isRecording = props.recordingTarget?.kind === 'radialOuter' && props.recordingTarget.index === slot.index;
            return (
              <button
                key={slot.index}
                type="button"
                className={[
                  activeSlot === slot.index || isRecording ? 'radial-editor-row radial-editor-row--active' : 'radial-editor-row',
                  radialSlotGridClass(slot.index),
                ].join(' ')}
                disabled={props.busyKey === editingBusyKey}
                onMouseEnter={() => setActiveSlot(slot.index)}
                onMouseLeave={() => setActiveSlot(null)}
                onFocus={() => setActiveSlot(slot.index)}
                onBlur={() => setActiveSlot(null)}
                onClick={() => {
                  props.setRecordingTarget(
                    isRecording ? null : { kind: 'radialOuter', index: slot.index, busyKey: editingBusyKey, mode: 'multi' },
                  );
                }}
              >
                <span className="radial-editor-row__angle">{radialDirectionLabel(slot.index)}</span>
                <span className="radial-editor-row__state key-row">
                  {isRecording ? (
                    <>
                      <KeyToken>等待按键…</KeyToken>
                      <KeyToken soft>再次点击取消</KeyToken>
                    </>
                  ) : slot.keys.length > 0 ? (
                    slot.keys.map((key) => <KeyToken key={`${slot.index}:${key}`}>{key}</KeyToken>)
                  ) : (
                    <KeyToken soft>点击录入</KeyToken>
                  )}
                </span>
              </button>
            );
          })}
          <div className="radial-editor-center">
            <div className="radial-editor-center__copy">
              <div className="radial-editor-center__label">内环启用</div>
              <div className="radial-editor-center__hint">关闭后双指划动将直接作用于外环</div>
            </div>
            <Switch
              checked={props.action.radialInnerEnabled}
              disabled={innerEnabledBusy}
              ariaLabel="切换径向菜单内环"
              onChange={(enabled) => void props.runAction('radial:inner-enabled', () => setRadialInnerEnabled(enabled))}
            />
          </div>
        </div>
      </div>
    </Panel>
  );
}

function BindingList(props: {
  rows: BindingDto[];
  busyKey: string | null;
  runAction: RunAction;
  recordingTarget: RecordingTarget | null;
  setRecordingTarget: (value: RecordingTarget | null) => void;
}) {
  return (
    <div className={`binding-grid binding-grid--count-${Math.min(props.rows.length, 4)}`}>
      {props.rows.map((row) => (
        <BindingItem key={row.id} row={row} {...props} />
      ))}
    </div>
  );
}

function BindingItem(props: {
  row: BindingDto;
  busyKey: string | null;
  runAction: RunAction;
  recordingTarget: RecordingTarget | null;
  setRecordingTarget: (value: RecordingTarget | null) => void;
}) {
  const editingBusyKey = `binding:keys:${props.row.id}`;
  const canEditKeys = Boolean(props.row.editableKeys);
  const isRecording = props.recordingTarget?.kind === 'binding' && props.recordingTarget.bindingId === props.row.id;
  const tokens = canEditKeys ? (props.row.editableKeys ?? []) : actionKeys(props.row.currentAction);
  const displayTokens = gestureDisplayTokens(props.row, tokens);

  return (
    <div className="binding-card">
      <div className="binding-card__head">
        <div>
          <div className="binding-card__title">{bindingDisplayLabel(props.row)}</div>
        </div>
      </div>
      <button
        type="button"
        className={isRecording ? 'record-button record-button--active' : 'record-button'}
        disabled={!canEditKeys || props.busyKey === editingBusyKey}
        onClick={() => {
          if (!canEditKeys || props.busyKey === editingBusyKey) return;
          props.setRecordingTarget(isRecording ? null : { kind: 'binding', bindingId: props.row.id, busyKey: editingBusyKey, mode: 'multi' });
        }}
      >
        <span className="key-row">
          {isRecording ? (
            <>
              <KeyToken>等待按键…</KeyToken>
              <KeyToken soft>再次点击取消</KeyToken>
            </>
          ) : displayTokens.length > 0 ? (
            displayTokens.map((token, index) => (
              <KeyToken key={`${props.row.id}:${index}:${token.label}`} soft={token.soft}>{token.label}</KeyToken>
            ))
          ) : (
            <KeyToken soft>{canEditKeys ? '点击录入' : actionTitle(props.row.currentAction)}</KeyToken>
          )}
        </span>
      </button>
      {isRecording ? (
        <div className="special-action-popover" role="dialog" aria-label={`${bindingDisplayLabel(props.row)} 特殊动作`}>
          {props.row.specialActions.length > 0 ? (
            <>
              <div className="special-action-popover__title">特殊动作</div>
              <div className="special-action-popover__options">
                {props.row.specialActions.map((option) => (
              <button
                key={option.id}
                type="button"
                className={option.id === props.row.activeSpecialAction ? 'special-action-option special-action-option--active' : 'special-action-option'}
                onClick={() => {
                  props.setRecordingTarget(null);
                  void props.runAction(`binding:special:${props.row.id}`, () => setBindingSpecialAction(props.row.id, option.id));
                }}
              >
                {option.label}
              </button>
                ))}
              </div>
            </>
          ) : null}
          <button
            type="button"
            className="special-action-clear"
            onClick={() => {
              props.setRecordingTarget(null);
              void props.runAction(`binding:keys:${props.row.id}`, () => setBindingKeys(props.row.id, []));
            }}
          >
            清空键盘按键
          </button>
        </div>
      ) : null}
    </div>
  );
}

function useBindingSections(rows: BindingDto[]) {
  return useMemo(() => {
    const findBinding = (id: string) => rows.find((row) => row.id === id);
    const filterBindings = (ids: string[]) => ids.map(findBinding).filter((row): row is BindingDto => Boolean(row));
    const sections = [
      { key: 'pen', title: '笔', rows: filterBindings(['stylus:squeeze', 'stylus:double_tap']) },
      { key: 'tap', title: '点击', rows: filterBindings(['stylus:two_tap', 'stylus:three_tap', 'stylus:four_tap']) },
      { key: 'pan', title: '平移', rows: filterBindings(['gesture:three_pan']) },
      { key: 'pinch', title: '捏合', rows: filterBindings(['gesture:two_pinch']) },
      { key: 'rotate', title: '旋转', rows: filterBindings(['gesture:two_rotate']) },
      { key: 'swipe', title: <>速划 <Badge tone="accent">单指</Badge></>, rows: rows.filter((row) => row.id.startsWith('gesture:swipe:')) },
      { key: 'long-press', title: '长按', rows: rows.filter((row) => row.id.startsWith('gesture:long_press:')) },
    ];
    return sections.filter((section) => section.rows.length > 0);
  }, [rows]);
}

function radialSlotGridClass(index: number) {
  return `radial-editor-row--slot-${index}`;
}

function swapRadialInnerSlots(
  slots: Extract<ActionDto, { kind: 'advanced' }>['radialInnerSlots'],
  from: RadialInnerSlotId,
  to: RadialInnerSlotId,
): RadialInnerBindingsPayload {
  const payload = Object.fromEntries(slots.map((slot) => [slot.slot, slot.key])) as RadialInnerBindingsPayload;
  const fromKey = payload[from];
  payload[from] = payload[to];
  payload[to] = fromKey;
  return payload;
}

function actionKeys(action: ActionDto) {
  switch (action.kind) {
    case 'holdKey':
    case 'triggerChord':
      return action.keys;
    case 'advanced':
      return [];
  }
}

type DisplayToken = { label: string; soft?: boolean };

function gestureDisplayTokens(row: BindingDto, tokens: string[]): DisplayToken[] {
  const result: DisplayToken[] = tokens.map((label) => ({ label }));
  const special = row.specialActions.find((option) => option.id === row.activeSpecialAction);
  if (special && special.id !== 'none') result.push({ label: special.label, soft: true });
  return result;
}

function actionTitle(action: ActionDto) {
  switch (action.kind) {
    case 'holdKey':
      return '按住按键';
    case 'triggerChord':
      return '触发组合键';
    case 'advanced':
      return action.label;
  }
}

function bindingDisplayLabel(row: BindingDto) {
  if (row.id === 'stylus:squeeze') return '挤压';
  if (row.id === 'stylus:double_tap') return '笔双击';
  if (row.id === 'stylus:two_tap') return '双指';
  if (row.id === 'stylus:three_tap') return '三指';
  if (row.id === 'stylus:four_tap') return '四指';
  if (row.id === 'gesture:three_pan') return '三指';
  if (row.id === 'gesture:two_pinch') return '双指';
  if (row.id === 'gesture:two_rotate') return '双指';
  if (row.id === 'gesture:swipe:1:horizontal') return '横向';
  if (row.id === 'gesture:swipe:1:vertical') return '纵向';
  const longPress = row.id.match(/^gesture:long_press:(\d)$/);
  if (longPress) {
    const map: Record<string, string> = { '1': '一指', '2': '二指', '3': '三指', '4': '四指' };
    return map[longPress[1]] ?? longPress[1] + '指';
  }
  return row.label;
}
