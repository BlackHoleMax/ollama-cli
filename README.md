# ollama-cli

一个基于 Rust 的 Ollama 终端用户界面（TUI）客户端，提供交互式的聊天、模型管理和在线搜索功能。

## 功能特性

- **聊天界面** - 与已安装的 Ollama 模型进行实时流式对话
- **模型管理** - 浏览和选择本地已安装的模型
- **在线搜索** - 搜索和发现 Ollama 模型库中的可用模型
- **现代化 TUI** - 使用 ratatui 构建的流畅终端界面

## 依赖要求

- Rust 1.70 或更高版本
- Ollama 服务运行在 `http://localhost:11434`

## 安装

```bash
git clone https://github.com/BlackHoleMax/ollama-cli
cd ollama-cli
cargo install --path .
```

## 使用方法

启动应用：

```bash
ollama-cli
```

### 快捷键

| 按键 | 功能 |
|------|------|
| `Tab` | 切换标签页（Chat / Models / Search） |
| `q` | 退出应用 |

#### Chat 标签页

| 按键 | 功能 |
|------|------|
| `Enter` | 发送消息 |
| `j` / `k` | 向下/向上滚动消息 |
| `g` | 滚动到顶部 |
| `G` | 滚动到底部 |

#### Models 标签页

| 按键 | 功能 |
|------|------|
| `j` / `↓` | 向下选择 |
| `k` / `↑` | 向上选择 |
| `g` | 跳到第一个模型 |
| `G` | 跳到最后一个模型 |
| `Enter` | 使用选中的模型 |

#### Search 标签页

| 按键 | 功能 |
|------|------|
| `Enter` | 执行搜索（空查询加载热门模型） |
| `j` / `↓` | 向下选择 |
| `k` / `↑` | 向上选择 |
| `g` | 跳到第一个结果 |
| `G` | 跳到最后一个结果 |

## 开发

构建项目：

```bash
cargo build
```

运行开发版本：

```bash
cargo run
```

## 技术栈

- [ratatui](https://github.com/ratatui-org/ratatui) - TUI 框架
- [crossterm](https://github.com/crossterm-rs/crossterm) - 终端操作
- [reqwest](https://github.com/seanmonstar/reqwest) - HTTP 客户端
- [tokio](https://github.com/tokio-rs/tokio) - 异步运行时
- [serde](https://github.com/serde-rs/serde) - 序列化/反序列化

## 许可证

本项目采用 MIT 许可证。
