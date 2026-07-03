import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';
import type { PressureControlPointDto, PressureCurveDto } from '../lib/tauri';
import { setPressureCurve } from '../lib/tauri';
import { Button, Panel, PanelHeader, Badge } from './ui';

type PressurePresetKey = 'linear' | 'soft' | 'hard' | 's';

type PressurePreset = {
  key: PressurePresetKey;
  label: string;
  curve: PressureCurveDto;
};

export const PRESSURE_CURVE_PRESETS: PressurePreset[] = [
  { key: 'linear', label: '线性', curve: { controlPoint1: { x: 0.33, y: 0.33 }, controlPoint2: { x: 0.66, y: 0.66 } } },
  { key: 'soft', label: '轻柔', curve: { controlPoint1: { x: 0.1, y: 0.5 }, controlPoint2: { x: 0.5, y: 0.9 } } },
  { key: 'hard', label: '扎实', curve: { controlPoint1: { x: 0.5, y: 0.1 }, controlPoint2: { x: 0.9, y: 0.5 } } },
  { key: 's', label: 'S 型', curve: { controlPoint1: { x: 0.6, y: 0.1 }, controlPoint2: { x: 0.4, y: 0.9 } } },
];

export function PressureCurveCard(props: {
  curve: PressureCurveDto;
  busy: boolean;
  runAction: (key: string, action: () => Promise<unknown>) => Promise<void>;
}) {
  const [draft, setDraft] = useState<PressureCurveDto>(props.curve);
  const [draggingPoint, setDraggingPoint] = useState<'controlPoint1' | 'controlPoint2' | null>(null);
  const boardRef = useRef<SVGSVGElement | null>(null);
  const draftRef = useRef<PressureCurveDto>(props.curve);

  useEffect(() => {
    if (!draggingPoint) {
      setDraft(props.curve);
      draftRef.current = props.curve;
    }
  }, [draggingPoint, props.curve]);

  const activePreset = pressurePresetKeyForCurve(draft);
  const entry = Math.round(draft.controlPoint1.y * 100);
  const release = Math.round(draft.controlPoint2.y * 100);

  const commitCurve = async (curve: PressureCurveDto) => {
    await props.runAction('pressure', () => setPressureCurve(curve));
  };

  const pointFromClient = (clientX: number, clientY: number) => {
    const board = boardRef.current;
    if (!board) return null;
    const rect = board.getBoundingClientRect();
    return {
      x: clamp01((clientX - rect.left) / rect.width),
      y: clamp01(1 - (clientY - rect.top) / rect.height),
    };
  };

  const updatePoint = (pointKey: 'controlPoint1' | 'controlPoint2', nextPoint: PressureControlPointDto) => {
    setDraft((current) => {
      const next = normalizePressureCurve({ ...current, [pointKey]: nextPoint });
      draftRef.current = next;
      return next;
    });
  };

  const updateFromClientPoint = (clientX: number, clientY: number, pointKey: 'controlPoint1' | 'controlPoint2') => {
    const nextPoint = pointFromClient(clientX, clientY);
    if (!nextPoint) return;
    updatePoint(pointKey, nextPoint);
  };

  const finishDrag = async () => {
    if (!draggingPoint) return;
    const curve = normalizePressureCurve(draftRef.current);
    draftRef.current = curve;
    try {
      await commitCurve(curve);
    } finally {
      setDraggingPoint(null);
    }
  };

  const handlePointerDown = (pointKey: 'controlPoint1' | 'controlPoint2') => (event: ReactPointerEvent<SVGCircleElement>) => {
    if (props.busy) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    setDraggingPoint(pointKey);
  };

  const renderHandle = (pointKey: 'controlPoint1' | 'controlPoint2') => {
    const point = draft[pointKey];
    const cx = point.x * 100;
    const cy = 100 - point.y * 100;
    return (
      <g key={pointKey} className={draggingPoint === pointKey ? 'pressure-anchor pressure-anchor--active' : 'pressure-anchor'}>
        <circle cx={cx} cy={cy} r="11" className="pressure-handle-hitbox" onPointerDown={handlePointerDown(pointKey)} />
        <circle cx={cx} cy={cy} r="5.2" className="pressure-handle-ring" />
        <circle cx={cx} cy={cy} r="2.6" className="pressure-handle" />
      </g>
    );
  };

  return (
    <Panel className="pressure-card">
      <PanelHeader
        title={(
          <span className="pressure-title">
            压感曲线
            <Badge tone="accent">{pressureCurveSummary(draft)}</Badge>
          </span>
        )}
      />

      <div className="pressure-editor">
        <div className="pressure-control-panel">
          <div className="pressure-readout" aria-label="当前压感参数">
            <span>入笔</span>
            <strong>{entry}%</strong>
          </div>
          <div className="pressure-readout" aria-label="释放压感参数">
            <span>收笔</span>
            <strong>{release}%</strong>
          </div>
          <div className="pressure-guide">
            <strong>调节说明</strong>
            <span>曲线上移：低压力输入对应更高输出。</span>
            <span>曲线下移：高压力输入对应更低输出。</span>
          </div>

          <div className="pressure-preset-grid" role="tablist" aria-label="压感曲线预设">
            {PRESSURE_CURVE_PRESETS.map((preset) => {
              const active = activePreset === preset.key;
              return (
                <Button
                  key={preset.key}
                  type="button"
                  role="tab"
                  aria-selected={active}
                  className={active ? 'pressure-preset pressure-preset--active' : 'pressure-preset'}
                  disabled={props.busy}
                  onClick={() => {
                    const curve = normalizePressureCurve(preset.curve);
                    draftRef.current = curve;
                    setDraft(curve);
                    void commitCurve(curve);
                  }}
                >
                  {preset.label}
                </Button>
              );
            })}
          </div>
        </div>

        <div className="pressure-stage-shell">
          <div className="pressure-axis pressure-axis--y">映射输出</div>
          <div className="pressure-stage">
            <svg
              ref={boardRef}
              viewBox="0 0 100 100"
              className={props.busy ? 'pressure-board pressure-board--busy' : 'pressure-board'}
              onPointerMove={(event) => {
                if (!draggingPoint) return;
                event.preventDefault();
                updateFromClientPoint(event.clientX, event.clientY, draggingPoint);
              }}
              onPointerUp={(event) => {
                if (!draggingPoint) return;
                event.preventDefault();
                void finishDrag();
              }}
              onPointerCancel={(event) => {
                if (!draggingPoint) return;
                event.preventDefault();
                void finishDrag();
              }}
            >
              <path d="M0 100 L100 0" className="pressure-reference" />
              <path d="M0 75 H100 M0 50 H100 M0 25 H100 M25 0 V100 M50 0 V100 M75 0 V100" className="pressure-grid" />
              <path d={`M 0 100 C ${draft.controlPoint1.x * 100} ${100 - draft.controlPoint1.y * 100}, ${draft.controlPoint2.x * 100} ${100 - draft.controlPoint2.y * 100}, 100 0`} className="pressure-ghost" />
              <path d={pressureCurvePath(draft)} className="pressure-line" />
              <line x1="0" y1="100" x2={draft.controlPoint1.x * 100} y2={100 - draft.controlPoint1.y * 100} className="pressure-handle-line" />
              <line x1="100" y1="0" x2={draft.controlPoint2.x * 100} y2={100 - draft.controlPoint2.y * 100} className="pressure-handle-line" />
              {renderHandle('controlPoint1')}
              {renderHandle('controlPoint2')}
            </svg>
          </div>
          <div className="pressure-axis pressure-axis--x">笔尖压力</div>
        </div>
      </div>
    </Panel>
  );
}

export function pressureCurveSummary(curve: PressureCurveDto) {
  const preset = PRESSURE_CURVE_PRESETS.find((item) => pressureCurvesEqual(item.curve, curve));
  return preset ? preset.label : '自定义';
}

function pressurePresetKeyForCurve(curve: PressureCurveDto): PressurePresetKey | null {
  return PRESSURE_CURVE_PRESETS.find((item) => pressureCurvesEqual(item.curve, curve))?.key ?? null;
}

function pressureCurvesEqual(left: PressureCurveDto, right: PressureCurveDto) {
  return pressurePointEquals(left.controlPoint1, right.controlPoint1) && pressurePointEquals(left.controlPoint2, right.controlPoint2);
}

function pressurePointEquals(left: PressureControlPointDto, right: PressureControlPointDto) {
  return Math.abs(left.x - right.x) < 0.001 && Math.abs(left.y - right.y) < 0.001;
}

function normalizePressureCurve(curve: PressureCurveDto): PressureCurveDto {
  return {
    controlPoint1: { x: clamp01(curve.controlPoint1.x), y: clamp01(curve.controlPoint1.y) },
    controlPoint2: { x: clamp01(curve.controlPoint2.x), y: clamp01(curve.controlPoint2.y) },
  };
}

function pressureCurvePath(curve: PressureCurveDto) {
  const samples = Array.from({ length: 33 }, (_, index) => {
    const t = index / 32;
    const x = cubicBezier(t, 0, curve.controlPoint1.x, curve.controlPoint2.x, 1) * 100;
    const y = 100 - cubicBezier(t, 0, curve.controlPoint1.y, curve.controlPoint2.y, 1) * 100;
    return `${index === 0 ? 'M' : 'L'} ${x} ${y}`;
  });
  return samples.join(' ');
}

function cubicBezier(t: number, p0: number, p1: number, p2: number, p3: number) {
  const oneMinusT = 1 - t;
  return oneMinusT ** 3 * p0 + 3 * oneMinusT ** 2 * t * p1 + 3 * oneMinusT * t ** 2 * p2 + t ** 3 * p3;
}

function clamp01(value: number) {
  return Math.min(1, Math.max(0, value));
}
