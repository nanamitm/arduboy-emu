# Roadmap

`arduboy-emu` と `ProjectABE` の実装をソースコードで比較した結果と、今後の計画。
この文書の比較基準日は 2026-07-13。`arduboy-emu` は v0.8.1、ProjectABE は
v0.6.8（コミット `f46014b`）を対象にしている。機能の有無は公開サイトの挙動ではなく、
両リポジトリに存在する実装を基準とする。

## ProjectABE との機能比較

### CPU / デバッグ基盤

| 機能 | ProjectABE | arduboy-emu v0.8.1 | 差分・補足 |
|------|:---:|:---:|------|
| AVR 命令実行 | ✓（JS 命令デコーダ） | ✓（80+ 命令デコーダ） | 命令網羅は 32u4/328P とも確認済み。SREG フラグはデータシート監査・修正済み。ROM 単位の実機互換性は継続検証 |
| ELPM / 24-bit flash | ✗ | ✓ | arduboy-emu が上回る |
| JIT コンパイル | ✓ | ✗ | ProjectABE は命令列を JS にコンパイル |
| 実行用 Web Worker | ✓ | ✓ | Wasmコアを `emulator-worker.js` で実行。画面・音声は転送バッファでUIへ返却 |
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
| ウォッチドッグ (WDT) | △（WDR は no-op） | ✓ | arduboy-emu は WDR・タイムアウト・WDE リセット（MCUSR.WDRF）・WDIE 割り込みを実装 |
| USB Serial | ✓ | ✓ | arduboy-emu は `--serial` とコアの出力捕捉 |
| FX Flash (W25Q128, 16 MB) | ✗ | ✓ | arduboy-emu が上回る |
| RGB / TX / RX LED | ✓ | ✓ | 両者対応 |
| 外部センサー (HC-SR04) | ✓ | ✗ | ProjectABE の `externalPeriferals/HCSR04.js` |

### フロントエンド / 開発体験

| 機能 | ProjectABE | arduboy-emu v0.8.1 | 差分・補足 |
|------|:---:|:---:|------|
| ブラウザ版 | ✓ | ✓ | arduboy-emu は Rust/WASM + Canvas |
| ネイティブデスクトップ版 | ✓（Electron） | ✓（minifb / Qt6） | arduboy-emu は C++/Qt フロントエンドも提供 |
| モバイル向け配布 | ✓（Cordova Android） | △（PWA、Android Qt Quick Debug APK） | Web版はインストール可能。Qt版は実機ビルド済みだが、署名付きリリース／ストア配布は未実施 |
| キーボード・ゲームパッド | ✓ | ✓（Web / Qt6） | Web Gamepad API と Qt6 の XInput（十字キー / 左スティック、A/X、B/Y）に対応 |
| タッチ操作 | ✓ | ✓（Web） | 両者対応 |
| ドラッグ＆ドロップ ROM 読込 | ✓ | ✓（Qt / Web） | minifb 版はディレクトリブラウズで代替 |
| `.arduboy` 読込 | ✓ | ✓ | 両者対応 |
| URL から ROM 読込 | ✓ | ✓（Web の `?rom=<url>`、Android の `arduboy://`） | AndroidはHTTPS URLをダウンロードしてロード。Web版はCORSに依存 |
| GIF 録画 / PNG スクリーンショット | ✓ | ✓ | 両者対応 |
| EEPROM 永続化 | ✓ | ✓ | ファイル / IndexedDB |
| セーブ状態・巻き戻し | ✗ | ✓ | arduboy-emu が上回る |
| ゲーム一覧 | ✓（外部リポジトリ） | ✓（ローカル / Webカタログ） | Web版は ArduboyCollection を検索・カテゴリ絞込み・直接起動できる |
| 本体スキン切替 | ✓（Arduboy, Microcard, Pipboy, Tama 等） | ✓（Web / Qt6: Arduboy, Microcard, Tama, Pipboy 3000, Pipboy Mk IV） | 両クライアントで選択を保存し、筐体ボタン操作にも対応 |
| IDE、ソース編集、ビルド | ✓ | ✗ | ProjectABE は Cloud / Arduino IDE ローカルコンパイラを持つ |
| 実機書込み | ✓（AVRGirl、デスクトップ） | ✗ | ProjectABE が上回る |
| QR コード生成／受信 | ✓ | △（AndroidでProjectABE形式を受信） | QR自体はURLを格納し、Android標準カメラから `arduboy://` を受信してロード |

### 現状の要約

arduboy-emu はエミュレーションの対象範囲、FX Flash、PCD8544、セーブ状態／巻き戻し、
プロファイラ、GDB、ELF/DWARF、Web / Qt6 のスキンとゲームパッド対応に強みがある。一方
ProjectABE は Web Worker、オンラインゲーム一覧、IDE／コンパイル、実機書込み、Android
配布を持つ。

旧版に残っていた「Web 版なし」「ゲームパッドなし」「ドラッグ＆ドロップなし」
「スキンなし」および「ProjectABE の Timer4 未対応」は、現行ソースと一致しないため訂正した。

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

### v0.5.0 — ATmega328P / Gamebuino Classic ✅ 完了

- [x] `CpuType`、328P メモリマップ、割り込みベクタ、Timer2
- [x] 328P 固有のポート制限と Gamebuino ボタンマッピング
- [x] `--cpu 328p` と PCD8544 SPI ルーティング
- [x] Gamebuino Classic 実ゲーム群での回帰テスト（Gamebuino-Classic-Games-Compilation 50 本を `rom_smoke` で走査。CPU 自動判定で全 50 本 328P・panic / 未知命令 / ロード失敗 0・全描画。互換性修正は不要だった）

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
- [x] Web Gamepad API（十字キー / 左スティック、A/X、B/Y。キーボード・タッチと入力を合成）
- [x] オンラインゲームリポジトリブラウザ（ArduboyCollection: 検索・カテゴリ絞込み・直接起動・週次更新）
- [x] インストール可能なPWA（オフラインのエミュレータシェルとカタログ）
- [x] スキンシステムの基盤（Arduboy / Microcard / Tama、UI・URL指定・選択の保存）
- [x] 追加スキン（Pipboy 3000 / Pipboy Mk IV のCSS筐体レイアウト）
- [ ] 実機画像アセット、ゲーム別の縦画面レイアウト
- [x] Web Worker 化（UI / 音声の応答性を保つため）

### v1.0.0 — 安定版リリース

**目標**: 実機互換性を検証可能にし、再現性のある配布・公開を整える。

- [x] SREG フラグ計算のデータシート監査と修正（COM の H 保存、FMUL/FMULS/FMULSU のキャリー、スキップ命令のサイクル数）＋命令別ユニットテスト
- [x] 実 ROM スモークテスト（`examples/rom_smoke`、CPU 自動判定）。Arduboy 317 本 + Gamebuino Classic 50 本 = 計 367 本すべてが描画・panic / 未知命令 / ロード失敗 0（Chirp は起動スプラッシュ後に画面を消す "隠して使う" 嫌がらせアプリで、描画自体は正常。判定は「実行中に一度でも描画したか」を採用）。検出・修正したバグ: ①ADC 変換完了漏れ（`analogRead` ポーリングが停止しないハング）②Timer4 割り込みベクタアドレスの誤り（TOIE4 使用ゲームが __bad_interrupt→連続リセット）
- [x] CI 組込のゴールデン回帰テスト（`tests/rom_regression.rs`、自作の最小 ROM・外部/許諾 ROM 不要で `cargo test`＝CI に自動搭載）。カバー範囲: ①SSD1306（Arduboy/32u4）②PCD8544（Gamebuino Classic/328P）の両ディスプレイ経路（framebuffer FNV ハッシュ）③ボタン入力（PINF→RAM）④タイマ＋割り込み（Timer0 オーバーフロー ISR のカウント）⑤音声（Timer1 CTC トーン周波数）⑥EEPROM（EEAR/EEDR/EECR 書込み→読戻し）⑦FX flash（W25Q128 の SPI Read Data）。命令実行・SPI・ディスプレイルーティング・GPIO 入力・割り込みディスパッチ・タイマ・EEPROM・外部フラッシュの回帰をまとめて検出
- [x] 命令別セマンティクステストの拡充（`tests/instruction_semantics.rs`）。約90の固定コスト命令のサイクル数をデータシート値と一括照合（不一致 0）、LPM/ELPM のフラッシュ読み・Z 前進・範囲外 0 返し・ELPM の RAMPZ 桁上げ境界を検証。分岐/スキップの可変サイクルは cpu.rs 単体テストでカバー
- [x] CI に `cargo fmt --check`、`cargo clippy`、`cargo test`、WASM ビルドを追加
- [x] タグ起点の GitHub Actions による Windows / Linux / macOS パッケージ作成とドラフトリリース
- [x] GitHub Actions の成果物を実機・複数ROMで検証するリリース手順（[RELEASE_VERIFICATION.md](RELEASE_VERIFICATION.md)）。自動プリフライト（`cargo test` ＋ `rom_smoke` で Arduboy 317 / Gamebuino 50 を走査）、固定の検証用ROMセットの headless スナップショット、プラットフォーム別のインストール/起動チェック、実機比較チェックリスト、公開前サインオフを規定
- [ ] crates.io 公開（`arduboy-core`）
- [x] API ドキュメント整備（`cargo doc`）。クレートレベルの概要＋動作する使用例（doctest）、主要公開 API（`Arduboy` の各サブシステム・`Button`/`CpuType`/`DisplayType`、SREG 定数、`Memory`/`Cpu`/`Ssd1306`/`AudioBuffer` の主要メソッド）をドキュメント化。`cargo doc` は警告なしでビルド（`opcodes`/`savestate` の機械的な内部表現は自明のため据え置き）
- [x] Cloudflare Pages（`arduboy-web`）への Web 版ホスティング
- [x] CORS を含む ROM URL 読込方針の明文化（[web/README.md](web/README.md) の「ROM loading & CORS policy」）。`?rom=` / カタログは静的サイト上のブラウザ `fetch()` で CORS に従う（GitHub raw / jsDelivr / 同一オリジンは可、CORS 非対応ホストは不可、`http://` は mixed-content でブロック）。プロキシは設けない方針（オープンリレー化・プライバシー回避）を明記し、取得失敗時のメッセージにも CORS / mixed-content のヒントを追加

### v1.x 以降の候補

- [ ] HC-SR04 など、利用実績のある外部ペリフェラルの追加
- [x] Android Qt Quick版の実機ビルド、ローカルファイル選択、`arduboy://` ROMリンク受信
- [ ] Android版のゲームパッド検証、署名付きRelease APK/AAB、ストア配布
- [ ] オンラインカタログ（ProjectABE / ArduboyCollection 互換を含む）
- [ ] IDE・コンパイル・実機書込みは、エミュレータ本体に統合するか外部ツール連携にするかを設計してから着手
