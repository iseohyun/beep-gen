// 버전 이원화 (.agents/rules/project_architecture.md 3절)
//
// BUILD_TAG  : 일자 기반 빌드 태그(YYYYMMDDvN). 웹에 반영되는 변경을 할 때마다 갱신한다.
//              같은 날 다시 바꾸면 N 을 올린다. 서비스 워커의 캐시 이름으로 쓰여
//              새로고침 없이도 최신 파일이 반영되게 한다.
// APP_VERSION: 정식 버전(SemVer). 사용자 승인 후에만 올리며 Cargo 작업 공간 버전과
//              같아야 한다(xtask 가 빌드 때 검사한다).
export const BUILD_TAG = "20261115v8";
export const APP_VERSION = "0.1.0";

declare const __BUILD_STAMP__: string;
/**
 * 빌드한 시각(월일-시분). 빌드할 때마다 자동으로 바뀐다(vite.config.ts).
 * 커밋하기 전의 작업본을 시험할 때, 지금 보는 화면이 방금 빌드한 것인지 확인하는 데 쓴다.
 */
export const BUILD_STAMP: string = __BUILD_STAMP__;
/** 화면과 제목표시줄에 보여주는 전체 버전 표기 (예: `0.1.0+20261115v4.1115-1630`). */
export const FULL_VERSION = `${APP_VERSION}+${BUILD_TAG}.${BUILD_STAMP}`;
