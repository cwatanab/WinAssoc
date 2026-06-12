# WinAssoc 仕様書（ドラフト v0.1）

Windows のファイル関連付け（拡張子）と URL プロトコルを、ルールベースで柔軟にルーティングする Rust 製ツール。

## 1. コンセプト

- **シム (プロキシ) exe 方式**: 自前の軽量 exe を拡張子・プロトコルのハンドラとして一度だけ登録し、起動のたびにルールを評価して実際のアプリへディスパッチする。
  - Windows 10/11 の UserChoice ハッシュ保護を毎回相手にする必要がなく、関連付けの変更は初回登録時のみ。
  - 条件分岐（パス・修飾キー・URL パターン）はシム側のロジックなので自由に拡張できる。
- **対象はファイルだけでなく URL も**: シムを既定ブラウザ（http/https ハンドラ）および任意の URI スキームのハンドラとして登録し、ブラウザ振り分けを行う。
- **CLI + 設定ファイル (TOML)**: ルールはテキストで定義し Git 管理・複数マシン同期が可能。GUI は選択ダイアログ（ピッカー）のみ最小限。
- **スコープは HKCU のみ**: 管理者権限不要。HKLM には書き込まない。

## 2. ルーティング対象とルール条件

| 対象 | 条件 |
|---|---|
| 拡張子 (.md, .pdf, …) | ファイルパスの glob、修飾キー (Shift/Ctrl)、選択ダイアログ |
| http / https | ホスト・URL パターン、修飾キー、選択ダイアログ |
| 任意の URI スキーム (mailto:, slack: 等) | 同上 |

- ルールは**上から順に評価し、最初に一致したものを採用**（first-match wins）。
- どのルールにも一致しない場合は、その拡張子/プロトコルの `default` アプリで開く（フォールバック）。
- `default` も未定義なら選択ダイアログを表示する。
- ブラウザ起動時は**プロファイル指定**（Chrome/Edge の `--profile-directory`、Firefox の `-P`）をアプリ定義の引数として表現できる。

### スコープ外（将来候補）

- ファイル内容（マジックバイト / shebang）による判定
- URL 書き換え（トラッキングパラメータ除去、Teams/Zoom リンクのネイティブ URI 変換）
- HKLM / 全ユーザー適用、ポータブル運用

## 3. 設定ファイル

場所: `%APPDATA%\winassoc\config.toml`（`--config` で上書き可）。

```toml
# ── アプリ定義 ──────────────────────────
[apps.vscode]
cmd  = 'C:\Users\cwatanab\AppData\Local\Programs\Microsoft VS Code\Code.exe'
args = ['{target}']            # {target} = ファイルパス or URL に展開

[apps.obsidian]
cmd  = '%LOCALAPPDATA%\Obsidian\Obsidian.exe'   # 環境変数展開をサポート
args = ['{target}']

[apps.chrome-work]
cmd  = 'C:\Program Files\Google\Chrome\Application\chrome.exe'
args = ['--profile-directory=Profile 1', '{target}']

[apps.firefox-personal]
cmd  = 'C:\Program Files\Mozilla Firefox\firefox.exe'
args = ['-P', 'personal', '{target}']

# ── 拡張子ルーティング ──────────────────
[ext.md]
default = "obsidian"
rules = [
  { glob = 'D:/Develop/**',  app = "vscode" },
  { modifier = "shift",      app = "vscode" },   # Shift+ダブルクリックでエディタ
]

[ext.html]
default = "firefox-personal"
rules = [
  { glob = '**/docs/**', app = "chrome-work" },
  { modifier = "ctrl",   pick = true },          # Ctrl 押下時はピッカー表示
]

# ── プロトコルルーティング ────────────────
[protocol.https]   # http も同じテーブルを共有するエイリアス設定可
default = "firefox-personal"
rules = [
  { host = '*.corp.example.com', app = "chrome-work" },
  { url  = 'https://github.com/myorg/**', app = "chrome-work" },
]

[protocol.mailto]
default = "outlook"
```

ルール 1 エントリの条件キー（`glob` / `host` / `url` / `modifier`）は**複数指定で AND**。アクションは `app = "name"` か `pick = true`（候補を絞った選択ダイアログ）。

## 4. CLI コマンド体系

2 バイナリ構成（同一 crate からビルド）:

- `winassoc.exe` — CLI (console subsystem)。管理・デバッグ用
- `winassoc-open.exe` — **シム本体** (GUI subsystem)。OS のハンドラとして登録され、コンソールを出さずに起動する。エラーはダイアログ + ログで通知

| コマンド | 役割 |
|---|---|
| `winassoc open <path\|url>` | シムと同じ評価を CLI から実行 (デバッグ用) |
| `winassoc apply` | 設定中の拡張子・プロトコルの ProgID/ハンドラ登録を HKCU に書き込み（冪等） |
| `winassoc unregister [ext\|all]` | 登録解除 |
| `winassoc list` | 設定済みルートと現在のレジストリ登録状態を一覧 |
| `winassoc test <path\|url>` | **dry-run**: どのルールに一致し何が起動されるかを表示（起動しない） |
| `winassoc doctor` | 設定とレジストリの乖離検出・修復提案（他アプリによる関連付け奪取、UserChoice が別アプリを指している等） |
| `winassoc log [--tail N]` | 起動ログ表示 |
| `winassoc backup` / `restore` | 登録前の関連付け状態を保存・復元（アンインストール時にも利用） |

## 5. レジストリ登録方式（HKCU）

- **ProgID**: `HKCU\Software\Classes\WinAssoc.<ext>` を作成し、`shell\open\command` に `"...\winassoc-open.exe" "%1"` を設定。`HKCU\Software\Classes\.<ext>` の既定値と `OpenWithProgids` に追加（既定値の元の内容は backup に保存し、unregister で復元）。
- **プロトコル**: 未登録スキームは `HKCU\Software\Classes\<scheme>`（`URL Protocol`）として直接登録。
- **既定ブラウザ**: `HKCU\Software\Clients\StartMenuInternet\WinAssoc` + `RegisteredApplications` に Capabilities（http/https/.html 等）を登録。
- **UserChoice の制約**: UserChoice ハッシュは偽造しない。`apply` 後、既定アプリの最終確定が必要なものは `ms-settings:defaultapps` を開いてユーザーに 1 クリックしてもらう（doctor が未確定項目を検出して案内）。
- 変更後は `SHChangeNotify(SHCNE_ASSOCCHANGED)` でシェルに通知。

## 6. シムの動作要件

- **起動レイテンシ目標**: ルール一致でそのまま起動するパスは < 50ms（ダブルクリック→アプリ起動の体感に直結）。ピッカー表示パスは < 150ms。重い依存を避け、設定パースを高速に保つ。
- 修飾キーは起動直後に `GetAsyncKeyState` で検出。
- 起動失敗時（アプリ消失等）はエラーダイアログ＋ログ記録し、フォールバックがあればそちらを試行。
- ログ: `%LOCALAPPDATA%\winassoc\logs\` にローテーション付きで「日時 / 対象 / 一致ルール / 起動アプリ / 結果」を記録。

## 6.5 ピッカー（ランチャー画面）

候補アプリをアイコン付きで横一列に並べる、ランチャー風の選択 UI。

```
╭──────────────────────────────────────╮
│   report.md を開くアプリを選択          │
│                                      │
│  ┏━━━━━━┓  ┌──────┐  ┌──────┐      │
│  ┃ [icon]┃  │[icon]│  │[icon]│      │
│  ┗━━━━━━┛  └──────┘  └──────┘      │
│   VS Code    Obsidian   Chrome       │
│     [1]        [2]    (Profile 1)[3] │
╰──────────────────────────────────────╯
  ←/→ 選択 · Enter/数字で決定 · Esc で中止
```

### レイアウト・操作

- **アイコン横並びグリッド**（候補 2〜6 想定。あふれた場合は横スクロールせず 2 段に折り返し）。
- アプリ名＋補足ラベル（プロファイル名等）をアイコン下に表示。
- キーボード: `←/→` で選択移動、`1`〜`9` で即決定、`Enter` で決定、`Esc` でキャンセル（何も起動しない）。
- マウス: ホバーで選択、クリックで決定。フォーカスを失ったら自動的に閉じる（キャンセル扱い）。

### ビジュアル

- **Mica/Acrylic 半透明背景 + 角丸**: `DwmSetWindowAttribute` の `DWMWA_SYSTEMBACKDROP_TYPE` / `DWMWA_WINDOW_CORNER_PREFERENCE` で OS ネイティブのマテリアルを適用。枠なし (borderless) ウィンドウ。
- **ダーク/ライト自動追従**: OS のアプリテーマ設定 (`AppsUseLightTheme`) を読んで配色を切り替え。
- **ホバー/選択アニメーション**: 選択枠のスライド移動、ホバー時のアイコン軽微拡大などの短い (≦150ms) イージング付きモーション。
- **表示位置はマウスカーソル付近**: ダブルクリック地点の近くにポップアップ。モニタのワークエリア内にクランプし、はみ出さない。

### アプリアイコン取得

- 各アプリ定義の `cmd` の exe から `IShellItemImageFactory`（64px、`SIIGBF_ICONONLY`）で抽出。取得失敗時は頭文字のジェネリックタイルにフォールバック。
- 抽出は数 ms/個と十分高速なためディスクキャッシュは設けない（レイテンシ目標を割る場合に将来導入）。

## 7. 技術スタック（案）

- レジストリ: `winreg` または `windows` crate
- CLI: `clap`
- 設定: `toml` + `serde`
- glob: `globset`
- ログ: `tracing` + ファイルローテーション
- ピッカー UI: `eframe` (egui)。Mica/Acrylic は追加クレートなしで `DwmSetWindowAttribute` を直接呼んで適用。CJK グリフのためシステムフォント (Yu Gothic 等) を実行時にロード
- アイコン抽出: `windows` crate 経由の `IShellItemImageFactory`

## 8. 未決事項

- ツール名（仮: WinAssoc）
- ピッカーで「最近使った候補を先頭に並び替える」かどうか（履歴の永続化が必要になる）
- `protocol.http` と `https` の共有指定の文法
- 運用機能 4 種（doctor / test / log / backup）の優先順位 — 全部入り想定だが削減可
