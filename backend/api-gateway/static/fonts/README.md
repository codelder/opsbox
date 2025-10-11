将 Google Sans Code 字体文件放到本目录（被 SvelteKit 作为静态资源 /fonts 提供）。

注意（License）：
- 请确保你拥有 Google Sans Code 的合法使用与自托管许可。
- 若无许可，建议使用开源的替代字体（如 JetBrains Mono / Fira Code / Roboto Mono）。

建议文件命名（与项目内 @font-face 对应）：
- google-sans-code-regular.woff2  （400）
- google-sans-code-medium.woff2   （500）
- google-sans-code-bold.woff2     （700）

放置后，访问路径为：
- /fonts/google-sans-code-regular.woff2
- /fonts/google-sans-code-medium.woff2
- /fonts/google-sans-code-bold.woff2

如果你使用不同的文件名，请同步修改 src/app.css 中的 @font-face src 路径。
