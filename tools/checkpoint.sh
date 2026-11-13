#!/usr/bin/env bash
# 작업 폴더의 현재 상태를 체크포인트로 저장한다. 브랜치·스테이징 영역은 건드리지 않는다.
#
#   tools/checkpoint.sh <이름> "<설명: 끝난 것 / 남은 것>"
#
# 저장 위치: refs/checkpoints/<이름>  (커밋되지 않은 변경과 새 파일 포함, .gitignore 대상 제외)
# 목록 보기:  git log --oneline refs/checkpoints/<이름>
# 되살리기:   git checkout refs/checkpoints/<이름> -- .
# 지우기:     git update-ref -d refs/checkpoints/<이름>
#
# 세션이 갑자기 끊겨도 마지막 체크포인트부터 이어서 작업할 수 있게 하려는 용도다.
# 정식 커밋이 아니므로 푸시되지 않는다.
set -euo pipefail

name="${1:?체크포인트 이름이 필요합니다}"
message="${2:?설명이 필요합니다}"
ref="refs/checkpoints/${name}"

cd "$(git rev-parse --show-toplevel)"

# 실제 스테이징 영역 대신 임시 영역을 써서 현재 파일 전체의 스냅샷을 만든다.
GIT_INDEX_FILE="$(git rev-parse --git-dir)/checkpoint.index"
export GIT_INDEX_FILE
trap 'rm -f "$GIT_INDEX_FILE"' EXIT
git read-tree HEAD
# 줄바꿈 변환 안내는 스냅샷과 무관하므로 숨긴다.
git add -A -- . ':!.claude' 2>/dev/null
tree="$(git write-tree)"

# 이전 체크포인트가 있으면 그 뒤에 잇고, 없으면 현재 커밋에서 시작한다.
parent="$(git rev-parse -q --verify "$ref" || git rev-parse HEAD)"
if [ "$(git rev-parse "${parent}^{tree}")" = "$tree" ]; then
  echo "변경 없음: $ref ($(git rev-parse --short "$parent"))"
  exit 0
fi

commit="$(git commit-tree "$tree" -p "$parent" -m "checkpoint(${name}): ${message}")"
git update-ref "$ref" "$commit"
echo "저장됨: $ref ($(git rev-parse --short "$commit"))"
