// 화면 스레드와 워커가 주고받는 메시지 형식.

export type WorkerRequest =
  | { id: number; type: "describe" }
  | { id: number; type: "introText"; template: string; paramsJson: string }
  | {
      id: number;
      type: "identify";
      /** 음악 파일의 내용(태그를 읽는다). */
      music: ArrayBuffer | null;
      /** 음악 파일의 원래 이름. */
      sourceName: string | null;
    }
  | {
      id: number;
      type: "generate";
      paramsJson: string;
      format: string;
      /** 배경 음악 파일의 내용. 없으면 비프만 만든다. */
      music: ArrayBuffer | null;
      /** 결과 파일의 이름을 짓는 데 쓰는 정보(JSON): 가수·곡, 원래 파일 이름. */
      musicNameJson: string | null;
      /** 안내 음성 파일의 내용. 없으면 안내 음성을 넣지 않는다. */
      intro: ArrayBuffer | null;
    };

export type WorkerResponse =
  | { id: number; type: "described"; infoJson: string }
  | { id: number; type: "introText"; text: string }
  | { id: number; type: "identified"; guessJson: string }
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
