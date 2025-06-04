### 1. Error冲突
```shell
原因是你同时引入了：￼

这会导致 ‎`Error` 名字冲突，因为 Rust 不允许在同一个作用域里有两个同名类型。

解决方法 ￼

1. 用 ‎`as` 重命名其中一个 ￼

最常见做法是给其中一个 ‎`Error` 起别名：￼

然后你的函数签名和返回值就可以区分：
 • 用 ‎`ActixError` 代表 ‎`actix_web::Error`
 • 用 ‎`AnyhowError` 代表 ‎`anyhow::Error`
```
