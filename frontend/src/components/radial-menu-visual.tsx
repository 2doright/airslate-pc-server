import { useRef, useState, type PointerEvent } from 'react';
import type { ActionDto, RadialInnerSlotId } from '../lib/tauri';

const OUTER_LABEL_POINTS = [
  { x: 50, y: 9 },
  { x: 79, y: 21 },
  { x: 91, y: 50 },
  { x: 79, y: 79 },
  { x: 50, y: 91 },
  { x: 21, y: 79 },
  { x: 9, y: 50 },
  { x: 21, y: 21 },
];

const INNER_KEY_POINTS: Record<RadialInnerSlotId, { x: number; y: number }> = {
  top: { x: 50, y: 30 },
  right: { x: 70, y: 50 },
  bottom: { x: 50, y: 70 },
  left: { x: 30, y: 50 },
};

const DIRECTION_LABELS = ['上', '右上', '右', '右下', '下', '左下', '左', '左上'];
const DIRECTION_ANGLES = [-90, -45, 0, 45, 90, 135, 180, 225];

export function RadialMenuVisual(props: {
  action: Extract<ActionDto, { kind: 'advanced' }>;
  innerEnabled: boolean;
  activeSlot: number | null;
  onActiveSlotChange: (slot: number | null) => void;
  innerBusy: boolean;
  onInnerSwap: (from: RadialInnerSlotId, to: RadialInnerSlotId) => void;
}) {
  const slots = orderRadialSlots(props.action.radialOuterSlots);
  const visualRef = useRef<HTMLDivElement | null>(null);
  const [draggingInnerSlot, setDraggingInnerSlot] = useState<RadialInnerSlotId | null>(null);

  const handleInnerPointerDown = (slot: RadialInnerSlotId, event: PointerEvent<HTMLSpanElement>) => {
    if (props.innerBusy || event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    setDraggingInnerSlot(slot);
  };

  const handleInnerPointerUp = (event: PointerEvent<HTMLSpanElement>) => {
    if (!draggingInnerSlot || props.innerBusy || !visualRef.current) {
      setDraggingInnerSlot(null);
      return;
    }
    const targetSlot = innerSlotFromClientPoint(visualRef.current, event.clientX, event.clientY);
    if (targetSlot !== draggingInnerSlot) props.onInnerSwap(draggingInnerSlot, targetSlot);
    setDraggingInnerSlot(null);
  };

  return (
    <div className="radial-visual-card" aria-label="径向菜单：内环键位可拖动互换，外环 8 个方向">
      <div className="radial-visual" role="img" ref={visualRef}>
        <div className={props.innerEnabled ? 'radial-disc' : 'radial-disc radial-disc--inner-disabled'} aria-hidden="true" />
        {slots.map((slot, visualIndex) => {
          const point = OUTER_LABEL_POINTS[visualIndex];
          return (
            <span
              key={`label:${slot.index}`}
              className={props.activeSlot === slot.index ? 'radial-slot-angle radial-slot-angle--active' : 'radial-slot-angle'}
              style={{ left: `${point.x}%`, top: `${point.y}%` }}
              onMouseEnter={() => props.onActiveSlotChange(slot.index)}
              onMouseLeave={() => props.onActiveSlotChange(null)}
            >
              <span className="radial-slot-arrow" style={{ transform: `rotate(${radialDirectionAngle(slot.index)}deg)` }} aria-hidden="true">
                <span className="radial-slot-arrow__head" />
              </span>
              <span className="radial-slot-label" aria-hidden="true">{radialDirectionLabel(slot.index)}</span>
            </span>
          );
        })}
        {props.action.radialInnerSlots.map((slot) => {
          const point = INNER_KEY_POINTS[slot.slot];
          const dragging = draggingInnerSlot === slot.slot;
          return (
            <span
              key={slot.slot}
              className={[
                'radial-inner-key',
                dragging ? 'radial-inner-key--dragging' : '',
                props.innerBusy ? 'radial-inner-key--busy' : '',
                props.innerEnabled ? '' : 'radial-inner-key--disabled',
              ].filter(Boolean).join(' ')}
              style={{ left: `${point.x}%`, top: `${point.y}%` }}
              aria-label={`${slot.label}：${slot.key}${props.innerEnabled ? '，拖动交换键位' : '，当前已禁用，可拖动调整保存位置'}`}
              onPointerDown={(event) => handleInnerPointerDown(slot.slot, event)}
              onPointerUp={handleInnerPointerUp}
              onPointerCancel={() => setDraggingInnerSlot(null)}
            >
              {slot.key}
            </span>
          );
        })}
        <span className="radial-center-dot" aria-hidden="true" />
      </div>
    </div>
  );
}

export function radialDirectionLabel(index: number) {
  return DIRECTION_LABELS[index] ?? `方向 ${index + 1}`;
}

function radialDirectionAngle(index: number) {
  return DIRECTION_ANGLES[index] ?? 0;
}

function innerSlotFromClientPoint(element: HTMLElement, clientX: number, clientY: number): RadialInnerSlotId {
  const rect = element.getBoundingClientRect();
  const dx = clientX - (rect.left + rect.width / 2);
  const dy = clientY - (rect.top + rect.height / 2);
  if (Math.abs(dx) > Math.abs(dy)) return dx >= 0 ? 'right' : 'left';
  return dy >= 0 ? 'bottom' : 'top';
}

export function orderRadialSlots<T extends { index: number }>(slots: T[]) {
  return [...slots].sort((left, right) => left.index - right.index);
}
