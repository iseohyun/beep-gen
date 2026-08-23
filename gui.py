"""
PyQt6 GUI Module for Beep Audio Generator
- 다중 TTS 엔진(Edge-TTS, gTTS, Windows SAPI5) 및 성우 선택 지원
- 비트 5초 미리듣기 및 TTS 음성 즉시 미리듣기 기능
"""

import os
import sys
import winsound
from typing import Optional

from PyQt6.QtCore import Qt, QThread, pyqtSignal
from PyQt6.QtGui import QFont
from PyQt6.QtWidgets import (
    QApplication,
    QButtonGroup,
    QComboBox,
    QDoubleSpinBox,
    QFileDialog,
    QFormLayout,
    QGroupBox,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QMainWindow,
    QMessageBox,
    QProgressBar,
    QPushButton,
    QRadioButton,
    QSlider,
    QSpinBox,
    QVBoxLayout,
    QWidget,
)

import generator
import tts_engine


class WorkerThread(QThread):
    progress = pyqtSignal(int, str)
    finished = pyqtSignal(str)
    error = pyqtSignal(str)

    def __init__(self, params: dict):
        super().__init__()
        self.params = params

    def run(self):
        try:
            out_file = generator.build_audio_file(
                n=self.params["n"],
                m=self.params["m"],
                weak_volume_pct=self.params["weak_volume_pct"],
                length_sec=self.params["length_sec"],
                tts_template=self.params["tts_template"],
                tts_engine_name=self.params["tts_engine_name"],
                tts_voice=self.params["tts_voice"],
                output_path=self.params["output_path"],
                progress_callback=lambda p, msg: self.progress.emit(p, msg),
            )
            self.finished.emit(out_file)
        except Exception as e:
            self.error.emit(str(e))


class BeepGenMainWindow(QMainWindow):
    def __init__(self):
        super().__init__()
        self.setWindowTitle("Beep Audio Generator (비프음 생성기)")
        self.resize(600, 780)
        self.setMinimumSize(540, 700)

        self.preview_wav_path: Optional[str] = None
        self.is_preview_playing = False
        self.last_generated_file: Optional[str] = None

        self.init_ui()
        self.update_filename_recommendation()

    def init_ui(self):
        central_widget = QWidget()
        self.setCentralWidget(central_widget)
        main_layout = QVBoxLayout(central_widget)
        main_layout.setSpacing(12)
        main_layout.setContentsMargins(18, 18, 18, 18)

        # 1. 헤더 타이틀
        title_label = QLabel("🎵 맞춤형 비프음 생성기 (Beep Gen)")
        title_font = QFont("Segoe UI", 14, QFont.Weight.Bold)
        title_label.setFont(title_font)
        title_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(title_label)

        sub_desc = QLabel("원하는 박자와 쪼개기 비트를 설정하고 고품질 AI/시스템 음성 인트로를 포함한 음원을 만듭니다.")
        sub_desc.setStyleSheet("color: #666; font-size: 11px;")
        sub_desc.setAlignment(Qt.AlignmentFlag.AlignCenter)
        main_layout.addWidget(sub_desc)

        # 2. 비트 및 소리 설정 그룹
        beat_group = QGroupBox("1. 비트 및 볼륨 설정")
        beat_layout = QFormLayout(beat_group)
        beat_layout.setSpacing(10)

        # n: 기본 비트 주기 (소수 1자리 지원)
        self.n_spin = QDoubleSpinBox()
        self.n_spin.setDecimals(1)
        self.n_spin.setSingleStep(0.1)
        self.n_spin.setRange(0.1, 3600.0)
        self.n_spin.setValue(1.0)
        self.n_spin.setSuffix(" 초")
        self.n_spin.valueChanged.connect(self.on_param_changed)
        beat_layout.addRow("기본 비트 주기 (n):", self.n_spin)

        # m: 비트 쪼개기
        self.m_spin = QSpinBox()
        self.m_spin.setRange(1, 64)
        self.m_spin.setValue(2)
        self.m_spin.setSuffix(" 분할")
        self.m_spin.valueChanged.connect(self.on_param_changed)
        beat_layout.addRow("쪼개기 수 (m):", self.m_spin)

        # 볼륨 슬라이더 + 스핀박스
        vol_container = QWidget()
        vol_layout = QHBoxLayout(vol_container)
        vol_layout.setContentsMargins(0, 0, 0, 0)
        vol_layout.setSpacing(8)

        self.vol_slider = QSlider(Qt.Orientation.Horizontal)
        self.vol_slider.setRange(0, 100)
        self.vol_slider.setValue(35)

        self.vol_spin = QSpinBox()
        self.vol_spin.setRange(0, 100)
        self.vol_spin.setValue(35)
        self.vol_spin.setSuffix(" %")

        self.vol_slider.valueChanged.connect(self.vol_spin.setValue)
        self.vol_spin.valueChanged.connect(self.vol_slider.setValue)
        self.vol_spin.valueChanged.connect(self.on_param_changed)

        vol_layout.addWidget(self.vol_slider)
        vol_layout.addWidget(self.vol_spin)
        beat_layout.addRow("작은 비트 볼륨:", vol_container)

        main_layout.addWidget(beat_group)

        # 3. 음원 길이 및 TTS 인트로 설정 그룹
        audio_group = QGroupBox("2. 인트로 음성(TTS) 및 음원 길이")
        audio_layout = QFormLayout(audio_group)
        audio_layout.setSpacing(10)

        # l: 음원 길이
        len_container = QWidget()
        len_h_layout = QHBoxLayout(len_container)
        len_h_layout.setContentsMargins(0, 0, 0, 0)
        len_h_layout.setSpacing(8)

        self.len_spin = QSpinBox()
        self.len_spin.setRange(1, 86400)
        self.len_spin.setValue(300)
        self.len_spin.setSuffix(" 초")
        self.len_spin.valueChanged.connect(self.on_length_changed)

        self.len_str_label = QLabel("(5분 00초)")
        self.len_str_label.setStyleSheet("color: #0066cc; font-weight: bold;")

        len_h_layout.addWidget(self.len_spin)
        len_h_layout.addWidget(self.len_str_label)
        len_h_layout.addStretch()
        audio_layout.addRow("음원 재생 길이 (l):", len_container)

        # TTS 엔진 선택
        self.tts_engine_combo = QComboBox()
        for key, info in tts_engine.TTS_ENGINES.items():
            self.tts_engine_combo.addItem(info["name"], key)
        self.tts_engine_combo.currentIndexChanged.connect(self.on_tts_engine_changed)
        audio_layout.addRow("음성(TTS) 엔진:", self.tts_engine_combo)

        # TTS 성우 선택
        self.tts_voice_combo = QComboBox()
        audio_layout.addRow("성우 / 목소리:", self.tts_voice_combo)

        # TTS 인트로 텍스트 & 음성 미리듣기 버튼
        tts_text_container = QWidget()
        tts_text_layout = QHBoxLayout(tts_text_container)
        tts_text_layout.setContentsMargins(0, 0, 0, 0)
        tts_text_layout.setSpacing(6)

        self.tts_edit = QLineEdit("{n}초, {n}초 비프")
        self.tts_edit.setPlaceholderText("예: {n}초, {n}초 비프 (비워두면 TTS 생략)")
        self.tts_edit.textChanged.connect(self.on_param_changed)

        self.tts_preview_btn = QPushButton("🗣️ 음성 듣기")
        self.tts_preview_btn.setToolTip("선택된 성우로 인트로 문구를 미리 들어봅니다.")
        self.tts_preview_btn.clicked.connect(self.preview_tts_voice)

        tts_text_layout.addWidget(self.tts_edit)
        tts_text_layout.addWidget(self.tts_preview_btn)
        audio_layout.addRow("시작 메시지:", tts_text_container)

        main_layout.addWidget(audio_group)

        # 4. 출력 파일 설정 그룹
        out_group = QGroupBox("3. 저장 파일 설정")
        out_layout = QFormLayout(out_group)
        out_layout.setSpacing(10)

        # 포맷 라디오 버튼
        fmt_container = QWidget()
        fmt_layout = QHBoxLayout(fmt_container)
        fmt_layout.setContentsMargins(0, 0, 0, 0)

        self.fmt_mp3 = QRadioButton("MP3 (권장, 소용량)")
        self.fmt_wav = QRadioButton("WAV (무손실)")
        self.fmt_mp3.setChecked(True)

        self.fmt_group = QButtonGroup(self)
        self.fmt_group.addButton(self.fmt_mp3)
        self.fmt_group.addButton(self.fmt_wav)
        self.fmt_mp3.toggled.connect(self.on_format_changed)

        fmt_layout.addWidget(self.fmt_mp3)
        fmt_layout.addWidget(self.fmt_wav)
        fmt_layout.addStretch()
        out_layout.addRow("파일 형식:", fmt_container)

        # 저장 파일 경로
        path_container = QWidget()
        path_layout = QHBoxLayout(path_container)
        path_layout.setContentsMargins(0, 0, 0, 0)
        path_layout.setSpacing(6)

        self.path_edit = QLineEdit()
        self.browse_btn = QPushButton("폴더 선택...")
        self.browse_btn.clicked.connect(self.on_browse_file)

        path_layout.addWidget(self.path_edit)
        path_layout.addWidget(self.browse_btn)
        out_layout.addRow("저장 파일명:", path_container)

        main_layout.addWidget(out_group)

        # 5. 미리듣기 & 생성 컨트롤
        ctrl_layout = QVBoxLayout()
        ctrl_layout.setSpacing(8)

        btn_row = QHBoxLayout()
        btn_row.setSpacing(8)

        self.preview_btn = QPushButton("🔊 비트 미리듣기 (5초)")
        self.preview_btn.setFixedHeight(38)
        self.preview_btn.clicked.connect(self.toggle_preview)

        self.generate_btn = QPushButton("🚀 음원 파일 생성하기")
        self.generate_btn.setFixedHeight(38)
        self.generate_btn.setStyleSheet(
            "QPushButton { background-color: #2b78e4; color: white; font-weight: bold; font-size: 13px; border-radius: 4px; }"
            "QPushButton:hover { background-color: #1a62cc; }"
            "QPushButton:disabled { background-color: #a0c0f0; }"
        )
        self.generate_btn.clicked.connect(self.start_generation)

        btn_row.addWidget(self.preview_btn)
        btn_row.addWidget(self.generate_btn)
        ctrl_layout.addLayout(btn_row)

        # 프로그레스 바 & 상태 텍스트
        self.progress_bar = QProgressBar()
        self.progress_bar.setRange(0, 100)
        self.progress_bar.setValue(0)
        self.progress_bar.setTextVisible(True)
        ctrl_layout.addWidget(self.progress_bar)

        self.status_label = QLabel("준비 완료")
        self.status_label.setStyleSheet("color: #444; font-size: 12px;")
        self.status_label.setAlignment(Qt.AlignmentFlag.AlignCenter)
        ctrl_layout.addWidget(self.status_label)

        # 생성 완료 후 액션 버튼 (폴더 열기 / 재생)
        self.post_action_container = QWidget()
        post_layout = QHBoxLayout(self.post_action_container)
        post_layout.setContentsMargins(0, 0, 0, 0)
        post_layout.setSpacing(8)

        self.open_folder_btn = QPushButton("📁 저장 폴더 열기")
        self.open_folder_btn.clicked.connect(self.open_output_folder)

        self.play_file_btn = QPushButton("▶ 생성된 파일 재생")
        self.play_file_btn.clicked.connect(self.play_generated_file)

        post_layout.addWidget(self.open_folder_btn)
        post_layout.addWidget(self.play_file_btn)
        self.post_action_container.hide()
        ctrl_layout.addWidget(self.post_action_container)

        main_layout.addLayout(ctrl_layout)
        main_layout.addStretch()

        # 초기 성우 목록 로드
        self.on_tts_engine_changed(0)

    def on_tts_engine_changed(self, index: int):
        engine_key = self.tts_engine_combo.currentData()
        voices = tts_engine.get_available_voices(engine_key)
        self.tts_voice_combo.clear()
        for vid, label in voices:
            self.tts_voice_combo.addItem(label, vid)

    def preview_tts_voice(self):
        engine_key = self.tts_engine_combo.currentData()
        voice_id = self.tts_voice_combo.currentData()
        n = self.n_spin.value()
        n_str = f"{n:g}"
        m = self.m_spin.value()
        l = self.len_spin.value()
        raw_text = self.tts_edit.text().strip()
        text = raw_text.format(n=n_str, m=m, l=l) if raw_text else f"{n_str}초 비프음"

        self.status_label.setText("TTS 음성 합성 중...")
        QApplication.processEvents()

        wav_path = generator.generate_tts_preview_wav(text, engine_key, voice_id)
        if wav_path and os.path.exists(wav_path):
            winsound.PlaySound(wav_path, winsound.SND_ASYNC | winsound.SND_FILENAME)
            self.status_label.setText(f"음성 재생 중: \"{text}\"")
        else:
            QMessageBox.warning(self, "TTS 안내", "음성 생성에 실패했습니다. 인터넷 연결이나 Windows 음성 설정을 확인해 주세요.")
            self.status_label.setText("TTS 음성 생성 실패")

    def on_length_changed(self, val: int):
        mins = val // 60
        secs = val % 60
        self.len_str_label.setText(f"({mins}분 {secs:02d}초)")
        self.update_filename_recommendation()

    def on_param_changed(self):
        self.update_filename_recommendation()

    def on_format_changed(self):
        self.update_filename_recommendation()

    def update_filename_recommendation(self):
        n = self.n_spin.value()
        n_str = f"{n:g}"
        m = self.m_spin.value()
        l = self.len_spin.value()
        ext = "mp3" if self.fmt_mp3.isChecked() else "wav"

        cur_text = self.path_edit.text().strip()
        dir_name = os.getcwd()
        if cur_text:
            dir_name = os.path.dirname(cur_text) or os.getcwd()

        filename = f"beep_n{n_str}s_m{m}_l{l}s.{ext}"
        self.path_edit.setText(os.path.join(dir_name, filename))

    def on_browse_file(self):
        ext = "mp3" if self.fmt_mp3.isChecked() else "wav"
        filter_str = "MP3 Files (*.mp3)" if ext == "mp3" else "WAV Files (*.wav)"

        file_path, _ = QFileDialog.getSaveFileName(
            self,
            "저장할 오디오 파일 선택",
            self.path_edit.text(),
            f"{filter_str};;All Files (*.*)",
        )
        if file_path:
            self.path_edit.setText(file_path)

    def toggle_preview(self):
        if self.is_preview_playing:
            winsound.PlaySound(None, winsound.SND_PURGE)
            self.is_preview_playing = False
            self.preview_btn.setText("🔊 비트 미리듣기 (5초)")
            self.status_label.setText("미리듣기 정지")
        else:
            n = self.n_spin.value()
            m = self.m_spin.value()
            vol = self.vol_spin.value()

            self.status_label.setText("미리듣기 오디오 생성 중...")
            QApplication.processEvents()

            try:
                self.preview_wav_path = generator.generate_preview_wav(
                    n=n, m=m, weak_volume_pct=vol, preview_len=5.0
                )
                winsound.PlaySound(self.preview_wav_path, winsound.SND_ASYNC | winsound.SND_FILENAME)
                self.is_preview_playing = True
                self.preview_btn.setText("⏹ 미리듣기 중지")
                self.status_label.setText(f"미리듣기 재생 중... (n={n}s, m={m}, vol={vol}%)")
            except Exception as e:
                QMessageBox.critical(self, "오류", f"미리듣기 실패: {e}")
                self.status_label.setText("미리듣기 오류")

    def start_generation(self):
        out_path = self.path_edit.text().strip()
        if not out_path:
            QMessageBox.warning(self, "경고", "저장할 파일 경로를 지정해 주세요.")
            return

        if self.is_preview_playing:
            self.toggle_preview()

        self.generate_btn.setEnabled(False)
        self.preview_btn.setEnabled(False)
        self.tts_preview_btn.setEnabled(False)
        self.post_action_container.hide()
        self.progress_bar.setValue(0)
        self.status_label.setText("생성 작업 시작 중...")

        params = {
            "n": self.n_spin.value(),
            "m": self.m_spin.value(),
            "weak_volume_pct": float(self.vol_spin.value()),
            "length_sec": self.len_spin.value(),
            "tts_template": self.tts_edit.text().strip(),
            "tts_engine_name": self.tts_engine_combo.currentData(),
            "tts_voice": self.tts_voice_combo.currentData(),
            "output_path": out_path,
        }

        self.worker = WorkerThread(params)
        self.worker.progress.connect(self.on_worker_progress)
        self.worker.finished.connect(self.on_worker_finished)
        self.worker.error.connect(self.on_worker_error)
        self.worker.start()

    def on_worker_progress(self, pct: int, msg: str):
        self.progress_bar.setValue(pct)
        self.status_label.setText(msg)

    def on_worker_finished(self, out_file: str):
        self.generate_btn.setEnabled(True)
        self.preview_btn.setEnabled(True)
        self.tts_preview_btn.setEnabled(True)
        self.last_generated_file = out_file
        self.progress_bar.setValue(100)
        self.status_label.setText(f"🎉 파일 생성 완료: {os.path.basename(out_file)}")
        self.post_action_container.show()
        QMessageBox.information(
            self,
            "생성 완료",
            f"성공적으로 음원 파일이 생성되었습니다!\n\n저장 경로:\n{out_file}",
        )

    def on_worker_error(self, err_msg: str):
        self.generate_btn.setEnabled(True)
        self.preview_btn.setEnabled(True)
        self.tts_preview_btn.setEnabled(True)
        self.status_label.setText("❌ 생성 중 오류 발생")
        QMessageBox.critical(self, "오류", f"음원 파일 생성 중 오류가 발생했습니다:\n{err_msg}")

    def open_output_folder(self):
        if self.last_generated_file and os.path.exists(self.last_generated_file):
            folder = os.path.dirname(os.path.abspath(self.last_generated_file))
            os.startfile(folder)
        else:
            cur_dir = os.path.dirname(os.path.abspath(self.path_edit.text()))
            if os.path.exists(cur_dir):
                os.startfile(cur_dir)

    def play_file_direct(self, file_path: str):
        if file_path and os.path.exists(file_path):
            os.startfile(file_path)

    def play_generated_file(self):
        if self.last_generated_file:
            self.play_file_direct(self.last_generated_file)

    def closeEvent(self, event):
        if self.is_preview_playing:
            winsound.PlaySound(None, winsound.SND_PURGE)
        if self.preview_wav_path and os.path.exists(self.preview_wav_path):
            try:
                os.remove(self.preview_wav_path)
            except OSError:
                pass
        event.accept()


def launch_gui():
    app = QApplication(sys.argv)
    window = BeepGenMainWindow()
    window.show()
    sys.exit(app.exec())


if __name__ == "__main__":
    launch_gui()
