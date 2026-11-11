/**
 * 后端类型定义。与 Rust 侧的 serde 结构一一对应。
 * 只描述形状，不含逻辑。
 */

export type TaskStatus =
  | "pending"
  | "running"
  | "done"
  | "failed"
  | "cancelled"
  | "incompatible";

export type MediaKind = "video" | "audio" | "image" | "subtitle";

export interface Task {
  id: number;
  sourcePath: string;
  fileName: string;
  sourceKind: MediaKind;
  sourceSummary: string;
  durationSecs: number;
  sourceSize: number;
  targetFormat: string;
  status: TaskStatus;
  percent: number;
  speed: number | null;
  outputPath: string;
  outputSize: number;
  message: string;
  notice: string;
}

export interface FormatView {
  id: string;
  name: string;
  kind: MediaKind;
  ext: string;
  animated: boolean;
  notes: string;
  available: boolean;
  unavailableReason: string;
}

export interface CapsView {
  encoderCount: number;
  muxerCount: number;
  ffmpegVersion: string;
}

export interface Settings {
  outputDir: string;
  overwriteExisting: boolean;
  concurrency: number;
  quality: number;
  audioBitrateKbps: number;
  maxHeight: number | null;
  stripMetadata: boolean;
  copyStreams: boolean;
  lastInputDir: string;
}

export interface Snapshot {
  tasks: Task[];
  formats: FormatView[];
  capabilities: CapsView;
  settings: Settings;
  ffmpegFound: boolean;
  isRunning: boolean;
}

export interface TaskUpdate {
  id: number;
  status: TaskStatus;
  percent: number;
  speed: number | null;
  outputPath: string;
  outputSize: number;
  message: string;
  notice: string;
}
