# Roadmap

`arduboy-emu` と `ProjectABE` の実装をソースコードで比較した結果と、今後の計画。
この文書の比較基準日は 2026-07-13。`arduboy-emu` は v0.8.1、ProjectABE は
v0.6.8（コミット `f46014b`）を対象にしている。機能の有無は公開サイトの挙動ではなく、
両リポジトリに存在する実装を基準とする。

## ProjectABE との機能比較

### CPU / デバッグ基盤

| 機能 | ProjectABE | arduboy-emu v0.8.1 | 差分・補足 |
|------|:---:|:---:|------|
| AVR 命令実行 | ✓（JS 命令デコーダ） | ✓（80+ 命令デコーダ） | 命令網羅性・実機互換性は継続検証が必要 |
| ELPM / 24-bit flash | ✗ | ✓ | arduboy-emu が上回る |
| JIT コンパイル | ✓ | ✗ | ProjectABE は命令列を JS にコンパイル |
| 実行用 Web Worker | ✓ | ✗ | ProjectABE は `Atcore.worker.js`、Web 版はメインスレッド実行 |
| ブレークポイント / ステップ実行 | ✓ | ✓ | 両者対応 |
| RAM read/write ウォッチポイント | ✓ | ✓ | arduboy-emu は値マッチも対応 |
| 逆アセンブラ | ✓ | ✓ | 両者対応 |
| ソース位置との対応 | ✓（コンパイラ生成 srcmap） | ✓（ELF/DWARF） | arduboy-emu は外部IDEを内蔵しない |
| ATmega328P / Gamebuino Classic | ✓ | ✓ | arduboy-emu は PCD8544 も実装 |
| 実行プロファイラ | ✗ | ✓ | PC ヒストグラム、ホットスポット、コールグラフ、CPI |
| GDB Remote Serial Protocol | ✗ | ✓ | `--gdb <port>` |
| I/O レジスタ名表示 | ✓ | ✓ | 32u4 / 328P を対象 |

### 表示・音声・ペリフェラル

| 機能 | ProjectABE | arduboy-emu v0.8.1 | 差分・補足 |
|------|:---:|:---:|------|
| SSD1306 OLED (128×64) | ✓ | ✓ | 両者対応 |
| PCD8544 / Gamebuino LCD | ✗ | ✓ | arduboy-emu が上回る |
| Invert / Contrast | ✓ | ✓ | 両者対応 |
| LCD 風エフェクト（残像・グリッド等） | ✓（スキン側表現） | ✓ | arduboy-emu は L キーで切替 |
| Timer1 / Timer3 CTC 音声 | ✓ | ✓ | 両者対応 |
| Timer4 高速 PWM 音声 | ✓ | ✓ | 旧ロードマップの ProjectABE「未対応」は誤り |
| GPIO ビットバング音声 | ✓ | ✓ | arduboy-emu はエッジをサンプル精度で記録 |
| ステレオ出力 | ✓ | ✓ | 両者対応 |
| Timer0/1/2/3/4、SPI、ADC、PLL、EEPROM | ✓ | ✓ | 両者対応 |
| USB Serial | ✓ | ✓ | arduboy-emu は `--serial` とコアの出力捕捉 |
| FX Flash (W25Q128, 16 MB) | ✗ | ✓ | arduboy-emu が上回る |
| RGB / TX / RX LED | ✓ | ✓ | 両者対応 |
| 外部センサー (HC-SR04) | ✓ | ✗ | ProjectABE の `externalPeriferals/HCSR04.js` |

### フロントエンド / 開発体験

| 機能 | ProjectABE | arduboy-emu v0.8.1 | 差分・補足 |
|------|:---:|:---:|------|
| ブラウザ版 | ✓ | ✓ | arduboy-emu は Rust/WASM + Canvas |
| ネイティブデスクトップ版 | ✓（Electron） | ✓（minifb / Qt6） | arduboy-emu は C++/Qt フロントエンドも提供 |
| モバイル向け配布 | ✓（Cordova Android） | △（レスポンシブUI・タッチ操作） | arduboy-emu に APK / PWA はない |
| キーボード・ゲームパッド | ✓ | ✓（デスクトップ） | Web Gamepad API は未実装 |
| タッチ操作 | ✓ | ✓（Web） | 両者対応 |
| ドラッグ＆ドロップ ROM 読込 | ✓ | ✓（Qt / Web） | minifb 版はディレクトリブラウズで代替 |
| `.arduboy` 読込 | ✓ | ✓ | 両者対応 |
| URL から ROM 読込 | ✓ | ✓（Web の `?rom=<url>`） | CORS は配信先に依存 |
| GIF 録画 / PNG スクリーンショット | ✓ | ✓ | 両者対応 |
| EEPROM 永続化 | ✓ | ✓ | ファイル / IndexedDB |
| セーブ状態・巻き戻し | ✗ | ✓ | arduboy-emu が上回る |
| ゲーム一覧 | ✓（外部リポジトリ） | ✓（ローカルディレクトリ） | オンラインリポジトリ統合は未実装 |
| 本体スキン切替 | ✓（Arduboy, Microcard, Pipboy, Tama 等） | ✗ | ProjectABE が上回る |
| IDE、ソース編集、ビルド | ✓ | ✗ | ProjectABE は Cloud / Arduino IDE ローカルコンパイラを持つ |
| 実機書込み | ✓（AVRGirl、デスクトップ） | ✗ | ProjectABE が上回る |
| QR コード生成 | ✓ | ✗ | ProjectABE はビルド成果物用 QR を生成 |

### 現状の要約

arduboy-emu はエミュレーションの対象範囲、FX Flash、PCD8544、セーブ状態／巻き戻し、
プロファイラ、GDB、ELF/DWARF に強みがある。一方 ProjectABE は Web Worker、スキン、
オンラインゲーム一覧、IDE／コンパイル、実機書込み、Android 配布を持つ。

旧版に残っていた「Web 版なし」「ゲームパッドなし」「ドラッグ＆ドロップなし」および
「ProjectABE の Timer4 未対応」は、現行ソースと一致しないため訂正した。

---

## バージョンアップ計画

### v0.2.0 — デバッグ基盤とディスプレイ強化 ✅ 完了

- [x] 逆アセンブラ、CLI ブレークポイント、ステップ実行
- [x] レジスタ / SREG / SP 表示、SSD1306 invert / contrast
- [x] 1×–6× スケール、フルスクリーン、スクリーンショット

### v0.3.0 — オーディオ改善と USB Serial ✅ 完了

- [x] 2ch ステレオ、サンプル精度波形バッファ
- [x] USB Serial 捕捉と `--serial`
- [x] Timer4（10-bit 高速 PWM）

### v0.4.0 — GUI フロントエンド ✅ 完了

- [x] `.arduboy` ZIP、EEPROM 永続化、GIF、PNG
- [x] LED 状態、FPS 切替、ローカルゲームブラウザ、ホットリロード
- [x] Qt 版での ROM ドラッグ＆ドロップ

### v0.5.0 — ATmega328P / Gamebuino Classic ✅ 実装完了・互換性検証継続

- [x] `CpuType`、328P メモリマップ、割り込みベクタ、Timer2
- [x] 328P 固有のポート制限と Gamebuino ボタンマッピング
- [x] `--cpu 328p` と PCD8544 SPI ルーティング
- [ ] Gamebuino Classic 実ゲーム群での回帰テストと互換性修正

### v0.6.0 — 高度なデバッグ機能と表示改善 ✅ 完了

- [x] RAM / I/O ビューア、read/write ウォッチポイント、値マッチ
- [x] 実行プロファイラ、GDB RSP サーバ
- [x] LCD エフェクト、ぼかし、整数スケーリング

### v0.7.0 — コア改善とデバッグ強化 ✅ 完了

- [x] ELF/DWARF、シンボルとソース位置の解決、`.elf` 直接読込
- [x] 巻き戻し、`--lcd`、`--no-blur`

### v0.8.0 — エコシステム統合と Web フロントエンド ✅ 中核実装完了

- [x] `wasm32-unknown-unknown` 向け `arduboy-wasm` バインディング
- [x] HTML/Canvas 表示、Web Audio AudioWorklet、キーボード・タッチ入力
- [x] `.hex` / `.arduboy` / FX 読込、URL パラメータ、ドラッグ＆ドロップ
- [x] IndexedDB による EEPROM とセーブ状態の永続化
- [x] Web 版の GIF / PNG、フルスクリーン、パレット、ポーズ / リセット
- [ ] Web Gamepad API
- [ ] オンラインゲームリポジトリブラウザ（ArduboyCollection 等）
- [x] スキンシステムの基盤（Arduboy / Microcard / Tama、UI・URL指定・選択の保存）
- [ ] 追加スキン、実機画像アセット、ゲーム別の縦画面レイアウト
- [ ] Web Worker 化（UI / 音声の応答性を保つため）

### v1.0.0 — 安定版リリース

**目標**: 実機互換性を検証可能にし、再現性のある配布・公開を整える。

- [ ] 未実装 AVR 命令・周辺機能の棚卸しと命令別テスト拡充
- [ ] Arduboy / Gamebuino の互換性テストROM・回帰テストスイート
- [ ] CI に `cargo fmt --check`、`cargo clippy`、`cargo test`、WASM ビルドを追加
- [x] タグ起点の GitHub Actions による Windows / Linux / macOS パッケージ作成とドラフトリリース
- [ ] GitHub Actions の成果物を実機・複数ROMで検証するリリース手順
- [ ] crates.io 公開（`arduboy-core`）
- [ ] API ドキュメント整備（`cargo doc`）
- [ ] Web 版の正式ホスティングと、CORS を含む ROM URL 読込方針の明文化

### v1.x 以降の候補

- [ ] HC-SR04 など、利用実績のある外部ペリフェラルの追加
- [ ] ProjectABE 互換のスキン / オンラインカタログ
- [ ] IDE・コンパイル・実機書込みは、エミュレータ本体に統合するか外部ツール連携にするかを設計してから着手
