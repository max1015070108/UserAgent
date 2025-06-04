### 部署
#### service
#### 依赖：
  + fluvio （docker 执行）
    https://github.com/infinyon/fluvio/tree/61c789d4188672ea303bc21c45b24c3e9702bf48/examples/docker-compose


  + mysql （docker 执行）


  + sqlite3 （本地执行）
  +


### login
```shell
### github
homepage:
http://localhost:3000

Authorization callback URL:
http://localhost:3000/auth/callback

### google
http://localhost:3000
http://localhost:3000/callback

```


### api分配方案

1. 登录/注册

github ： 注册及登录

google ： 注册及登录
email：   email，验证码，密码设置
phone：   验证码登录（最方便）


2. llm
+ 查询当前的模型有哪些？
+ 文/图生文模型请求
+ 文生图模型请求
+ 文生视频模型请求
+ 图生视频模型请求
