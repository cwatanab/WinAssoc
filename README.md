# winassoc

Windows のファイル関連付け（拡張子）と URL プロトコルを、ルールベースで柔軟にルーティングするツール。
詳細仕様は [SPEC.md](SPEC.md) を参照。

- **シム方式**: 軽量な `winassoc-open.exe` を一度だけハンドラ登録し、起動のたびにルールを評価して実アプリへディスパッチ
- **ファイルも URL も**: 拡張子ごとのルーティングに加え、既定ブラウザとして http/https や任意の URI スキームを振り分け
- **ルール条件**: パス glob / ホスト / URL パターン / 修飾キー (Shift/Ctrl/Alt) の AND 組み合わせ、first-match wins
- **ランチャー UI**: ルール未確定時はアプリアイコン付きのピッカーがカーソル位置にポップアップ (Acrylic 半透明・角丸・ダークモード追従)
- **HKCU のみ**: 管理者権限不要。UserChoice ハッシュは偽造せず、必要箇所のみ Windows 設定での 1 クリックを案内

## ビルドとインストール

```powershell
cargo build --release
# target\release\winassoc.exe      … CLI (管理用)
# target\release\winassoc-open.exe … シム本体 (OS から呼ばれる・コンソール非表示)
# 2 つの exe を同じディレクトリに置いて運用する
```

## クイックスタート

```powershell
# 1. 設定を書く
notepad $env:APPDATA\winassoc\config.toml   # 例は config.example.toml

# 2. ルールの動作確認 (dry-run、レジストリ変更なし)
winassoc test D:\Develop\notes\a.md
winassoc test https://github.com/myorg/repo --modifier shift

# 3. HKCU へ登録 (実行前に現状を自動バックアップ)
winassoc apply

# 4. 状態診断 (UserChoice の未確定項目もここで分かる)
winassoc doctor
```

http/https など UserChoice 保護された既定は、`apply` 後に `ms-settings:defaultapps` で
**WinAssoc** を選択して確定する（doctor が未確定項目を表示する）。

## コマンド

| コマンド | 役割 |
|---|---|
| `winassoc test <path\|url> [--modifier shift,ctrl]` | dry-run: 一致ルールと起動内容を表示 |
| `winassoc list` | 設定済みルートの一覧 |
| `winassoc apply` | HKCU へ登録 (冪等・自動バックアップ付き) |
| `winassoc unregister` | 登録解除 (作成したキーを掃除し、既定 ProgID を復元) |
| `winassoc doctor` | 設定とレジストリの乖離を診断 |
| `winassoc log [--tail N]` | 起動ログ表示 (`%LOCALAPPDATA%\winassoc\logs`) |
| `winassoc backup` / `restore [file]` | 関連付け状態の保存と復元 |
| `winassoc open <path\|url>` | シムと同じ評価を CLI から実行 (デバッグ用) |

## 開発

```powershell
cargo test                                   # ルールエンジンの単体テスト
winassoc apply --config test-config.toml    # 実関連付けに影響しない統合テスト用設定
```
