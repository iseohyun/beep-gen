// 화면 스레드와 워커가 주고받는 메시지 형식.

export type WorkerRequest =
  | { id: number; type: "describe" }
  | {
      id: number;
      type: "generate";
      paramsJson: string;
      format: string;
      /** 배경 음악 파일의 내용. 없으면 비프만 만든다. */
      music: ArrayBuffer | null;
    };

export type WorkerResponse =
  | { id: number; type: "described"; infoJson: string }
  | { id: number; type: "progress"; percent: number; stage: string }
  | {
      id: number;
      type: "generated";
      bytes: Uint8Array<ArrayBuffer>;
      fileName: string;
      /** 배경 음악을 넣었을 때의 보고(JSON). */
      musicReportJson: string | null;
    }
  | { id: number; type: "error"; message: string };
