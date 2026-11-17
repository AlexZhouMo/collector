// 通用进度条（工具箱长任务、设置页导入/导出共用）。
// 生成的元素 id：容器 `${id}`（默认隐藏）、填充条 `${id}-bar`、文字 `${id}-text`。
export const progressBar = (id: string, spacing = "margin-top:10px"): string =>
  `<div id="${id}" style="display:none;${spacing}">
    <div class="pbar-track"><div id="${id}-bar" class="pbar-fill"></div></div>
    <div id="${id}-text" class="pbar-text"></div>
  </div>`;
