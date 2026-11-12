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
  tune: Tune;
}

/**
 * 任务级参数细调。字段全为空表示「全用默认」。
 * 切换目标格式后，后端会把该格式不认识的字段清掉。
 */
export interface Tune {
  videoEncoder?: string | null;
  audioEncoder?: string | null;
  quality?: number | null;
  videoBitrateKbps?: number | null;
  preset?: string | null;
  maxWidth?: number | null;
  maxHeight?: number | null;
  fps?: number | null;
  audioBitrateKbps?: number | null;
  audioSampleRate?: number | null;
  audioChannels?: number | null;
  audioQuality?: number | null;
  gifFps?: number | null;
  gifWidth?: number | null;
  gifColors?: number | null;
  gifDither?: string | null;
  gifLoop?: boolean | null;
  subtitleFps?: number | null;
  imageQuality?: number | null;
}

export type ParamKind = "select" | "slider" | "number" | "toggle";

export interface ParamChoice {
  value: string;
  label: string;
}

/** 一条可调参数的描述，由后端按目标格式生成。 */
export interface ParamSpec {
  id: keyof Tune;
  label: string;
  /** 视频 / 音频 / 容器 / 图像 / 字幕 / 动图 */
  group: string;
  kind: ParamKind;
  default: string;
  choices: ParamChoice[];
  min: number | null;
  max: number | null;
  step: number | null;
  unit: string;
  help: string;
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
  lastInputDir: string;
}

export interface Snapshot {
  tasks: Task[];
  formats: FormatView[];
  capabilities: CapsView;
  settings: Settings;
  ffmpegFound: boolean;
  isRunning: boolean;
  appVersion: string;
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
