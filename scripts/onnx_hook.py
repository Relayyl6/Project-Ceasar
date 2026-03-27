import argparse
import hashlib
import json
import sys
from pathlib import Path


COCO80 = [
    "person", "bicycle", "car", "motorcycle", "airplane", "bus", "train", "truck", "boat", "traffic light",
    "fire hydrant", "stop sign", "parking meter", "bench", "bird", "cat", "dog", "horse", "sheep", "cow",
    "elephant", "bear", "zebra", "giraffe", "backpack", "umbrella", "handbag", "tie", "suitcase", "frisbee",
    "skis", "snowboard", "sports ball", "kite", "baseball bat", "baseball glove", "skateboard", "surfboard", "tennis racket", "bottle",
    "wine glass", "cup", "fork", "knife", "spoon", "bowl", "banana", "apple", "sandwich", "orange",
    "broccoli", "carrot", "hot dog", "pizza", "donut", "cake", "chair", "couch", "potted plant", "bed",
    "dining table", "toilet", "tv", "laptop", "mouse", "remote", "keyboard", "cell phone", "microwave", "oven",
    "toaster", "sink", "refrigerator", "book", "clock", "vase", "scissors", "teddy bear", "hair drier", "toothbrush",
]

SESSION = None
SESSION_INPUT_NAME = None
CLASS_LABELS = None


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="YOLOv8 ONNX detector hook for the Uriel edge node")
    parser.add_argument("--model", required=True, help="Path to the YOLOv8 ONNX model")
    parser.add_argument("--labels", help="Optional newline-delimited labels file")
    parser.add_argument("--imgsz", type=int, default=640, help="Square inference size")
    parser.add_argument("--conf-thres", type=float, default=0.25, help="Detection confidence threshold")
    parser.add_argument("--iou-thres", type=float, default=0.45, help="NMS IoU threshold")
    parser.add_argument("--providers", default="CPUExecutionProvider", help="Comma-separated onnxruntime providers")
    parser.add_argument("--scene-width-m", type=float, default=80.0, help="Approximate scene width for center projection")
    parser.add_argument("--scene-depth-m", type=float, default=50.0, help="Approximate scene depth for center projection")
    parser.add_argument("--primary-classes", nargs="*", default=["car", "truck", "bus", "motorcycle", "person"], help="Preferred classes when multiple detections exist")
    return parser.parse_args()


def load_runtime(args: argparse.Namespace):
    global SESSION
    global SESSION_INPUT_NAME
    global CLASS_LABELS

    if SESSION is not None:
        return

    try:
        import onnxruntime as ort
    except ImportError as exc:
        raise SystemExit("onnxruntime is required for scripts/onnx_hook.py") from exc

    SESSION = ort.InferenceSession(
        str(Path(args.model)),
        providers=[provider.strip() for provider in args.providers.split(",") if provider.strip()],
    )
    SESSION_INPUT_NAME = SESSION.get_inputs()[0].name
    CLASS_LABELS = load_labels(args.labels)


def load_labels(path: str | None) -> list[str]:
    if path:
        return [line.strip() for line in Path(path).read_text(encoding="utf-8").splitlines() if line.strip()]
    return COCO80


def main() -> int:
    args = parse_args()
    load_runtime(args)

    request = json.load(sys.stdin)
    frame_path = Path(request["frame_path"])
    frame_bytes = frame_path.read_bytes()
    digest = hashlib.blake2s(frame_bytes).hexdigest()

    image_tensor, meta = preprocess_image(frame_path, args.imgsz)
    outputs = SESSION.run(None, {SESSION_INPUT_NAME: image_tensor})
    detections = decode_outputs(outputs[0], meta, args, CLASS_LABELS)
    best = choose_detection(detections, args.primary_classes)

    if best is None:
        response = {
            "track_hint": f"no-target-{request['sequence'] % 32}",
            "confidence": 0.05,
            "class_label": "no_detection",
            "position_m": [args.scene_width_m / 2.0, args.scene_depth_m / 2.0],
            "velocity_mps": None,
            "evidence_digest": digest,
        }
    else:
        cx = (best["bbox"][0] + best["bbox"][2]) / 2.0
        cy = (best["bbox"][1] + best["bbox"][3]) / 2.0
        position_m = [
            round((cx / meta["orig_w"]) * args.scene_width_m, 3),
            round((cy / meta["orig_h"]) * args.scene_depth_m, 3),
        ]
        response = {
            "track_hint": f"{best['label']}-{request['sequence'] % 32}",
            "confidence": round(float(best["confidence"]), 5),
            "class_label": best["label"],
            "position_m": position_m,
            "velocity_mps": None,
            "evidence_digest": digest,
        }

    json.dump(response, sys.stdout)
    return 0


def preprocess_image(frame_path: Path, imgsz: int):
    try:
        import numpy as np
        from PIL import Image
    except ImportError as exc:
        raise SystemExit("numpy and Pillow are required for scripts/onnx_hook.py") from exc

    image = Image.open(frame_path).convert("RGB")
    orig_w, orig_h = image.size
    scale = min(imgsz / orig_w, imgsz / orig_h)
    resized_w = max(1, int(round(orig_w * scale)))
    resized_h = max(1, int(round(orig_h * scale)))
    resampling = getattr(Image, "Resampling", Image)
    resized = image.resize((resized_w, resized_h), resampling.BILINEAR)

    canvas = Image.new("RGB", (imgsz, imgsz), (114, 114, 114))
    pad_x = (imgsz - resized_w) // 2
    pad_y = (imgsz - resized_h) // 2
    canvas.paste(resized, (pad_x, pad_y))

    array = np.asarray(canvas, dtype=np.float32) / 255.0
    array = np.transpose(array, (2, 0, 1))[None, ...]

    return array, {
        "orig_w": orig_w,
        "orig_h": orig_h,
        "scale": scale,
        "pad_x": pad_x,
        "pad_y": pad_y,
    }


def decode_outputs(raw_output, meta: dict, args: argparse.Namespace, labels: list[str]) -> list[dict]:
    import numpy as np

    predictions = np.array(raw_output)
    if predictions.ndim == 3:
        predictions = predictions[0]

    if predictions.shape[0] in (84, 85) and predictions.shape[1] > predictions.shape[0]:
        predictions = predictions.T
    elif predictions.shape[1] in (84, 85):
        pass
    elif predictions.shape[0] > predictions.shape[1]:
        predictions = predictions
    else:
        predictions = predictions.T

    attr_count = predictions.shape[1]
    if attr_count < 6:
        raise SystemExit(f"unexpected YOLO output shape {predictions.shape}")

    boxes = predictions[:, :4]
    if attr_count == len(labels) + 5:
        objectness = predictions[:, 4]
        class_scores = predictions[:, 5:]
        scores = class_scores * objectness[:, None]
    else:
        class_scores = predictions[:, 4:]
        scores = class_scores

    class_ids = np.argmax(scores, axis=1)
    confidences = scores[np.arange(scores.shape[0]), class_ids]
    keep = confidences >= args.conf_thres
    if not np.any(keep):
        return []

    boxes = boxes[keep]
    class_ids = class_ids[keep]
    confidences = confidences[keep]

    xyxy = xywh_to_xyxy(boxes)
    xyxy[:, [0, 2]] -= meta["pad_x"]
    xyxy[:, [1, 3]] -= meta["pad_y"]
    xyxy /= meta["scale"]
    xyxy[:, [0, 2]] = np.clip(xyxy[:, [0, 2]], 0, meta["orig_w"])
    xyxy[:, [1, 3]] = np.clip(xyxy[:, [1, 3]], 0, meta["orig_h"])

    keep_indices = nms(xyxy, confidences, args.iou_thres)
    detections = []
    for index in keep_indices:
        class_id = int(class_ids[index])
        label = labels[class_id] if 0 <= class_id < len(labels) else f"class_{class_id}"
        detections.append(
            {
                "label": label,
                "confidence": float(confidences[index]),
                "bbox": xyxy[index].tolist(),
            }
        )
    return detections


def xywh_to_xyxy(boxes):
    import numpy as np

    converted = np.copy(boxes)
    converted[:, 0] = boxes[:, 0] - boxes[:, 2] / 2.0
    converted[:, 1] = boxes[:, 1] - boxes[:, 3] / 2.0
    converted[:, 2] = boxes[:, 0] + boxes[:, 2] / 2.0
    converted[:, 3] = boxes[:, 1] + boxes[:, 3] / 2.0
    return converted


def nms(boxes, scores, iou_threshold: float):
    import numpy as np

    order = scores.argsort()[::-1]
    keep = []

    while order.size > 0:
        i = order[0]
        keep.append(i)
        if order.size == 1:
            break

        xx1 = np.maximum(boxes[i, 0], boxes[order[1:], 0])
        yy1 = np.maximum(boxes[i, 1], boxes[order[1:], 1])
        xx2 = np.minimum(boxes[i, 2], boxes[order[1:], 2])
        yy2 = np.minimum(boxes[i, 3], boxes[order[1:], 3])

        w = np.maximum(0.0, xx2 - xx1)
        h = np.maximum(0.0, yy2 - yy1)
        intersection = w * h

        area_i = (boxes[i, 2] - boxes[i, 0]) * (boxes[i, 3] - boxes[i, 1])
        area_rest = (boxes[order[1:], 2] - boxes[order[1:], 0]) * (
            boxes[order[1:], 3] - boxes[order[1:], 1]
        )
        union = area_i + area_rest - intersection
        iou = np.divide(intersection, union, out=np.zeros_like(intersection), where=union > 0)

        remaining = np.where(iou <= iou_threshold)[0]
        order = order[remaining + 1]

    return keep


def choose_detection(detections: list[dict], primary_classes: list[str]) -> dict | None:
    if not detections:
        return None

    priority = {label: index for index, label in enumerate(primary_classes)}

    return sorted(
        detections,
        key=lambda detection: (
            priority.get(detection["label"], len(priority)),
            -detection["confidence"],
        ),
    )[0]


if __name__ == "__main__":
    raise SystemExit(main())
