pub const JWT_SECRET: &[u8] = b"your_super_secret_key"; // 生产环境请安全存储
pub const REDIRECT_URL: &str = "http://localhost:3000";

//google url
pub const GOOGLE_AUTH: &str = "https://accounts.google.com/o/oauth2/v2/auth";

pub const GOOGLE_AUTH_TOKEN: &str = "https://www.googleapis.com/oauth2/v3/token";
pub const GOOGLE_USER_INFO_URL: &str = "https://www.googleapis.com/oauth2/v2/userinfo";

//github url
pub const GITHUB_AUTH: &str = "https://github.com/login/oauth/authorize";

pub const GITHUB_AUTH_TOKEN: &str = "https://github.com/login/oauth/access_token";
pub const GITHUB_USER_INFO_URL: &str = "https://api.github.com/user";
