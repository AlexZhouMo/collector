//! 展示图导入：把用户选的图拷到应用数据目录下的 covers/，返回目标路径。
//! 不引用原图路径（原图移动/删除不影响封面），供 cover_path 存储。
//!
//! 注：非裁剪版 import_cover 已随「无前端入口的封面导入命令」一并删除；
//! 当前裁剪导入逻辑（import_cover_cropped / cover_subdir）位于 lib.rs。
