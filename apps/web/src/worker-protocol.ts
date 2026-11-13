// 화면 스레드와 워커가 주고받는 메시지 형식.

export type WorkerRequest =
  | { id: number; type: "describe" }
  | { id: number; type: "generate"; paramsJson: string; format: string };

export type WorkerResponse =
  | { id: number; type: "described"; infoJson: string }
  | { id: number; type: "progress"; percent: number }
  | { id: number; type: "generated"; bytes: Uint8Array<ArrayBuffer>; fileName: string }
  | { id: number; type: "error"; message: string };
