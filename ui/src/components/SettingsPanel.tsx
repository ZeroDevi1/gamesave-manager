// components/SettingsPanel.tsx - 统一存储多后端设置面板 (直连网盘/自建Alist/WebDAV/S3 适配面板)
import {
  Button,
  Input,
  Label,
  RadioGroup,
  Radio,
  makeStyles,
  tokens,
  Spinner,
  MessageBar,
  MessageBarBody,
} from '@fluentui/react-components'
import { useState, useEffect } from 'react'
import {
  loadConfig,
  saveConfig,
  alistLogin,
  storageTestConnection,
  storageRefreshAllTokens,
  quarkTvGetQrCode,
  quarkTvPollQr,
  quarkTvExchange,
} from '../services/tauri'
import type { StorageConfig } from '../services/tauri'
import { useAppStore } from '../store/appStore'
import StorageBrowser from './StorageBrowser'

const useStyles = makeStyles({
  root: {
    display: 'flex',
    flexDirection: 'column',
    gap: '20px',
    maxWidth: '600px',
  },
  section: {
    display: 'flex',
    flexDirection: 'column',
    gap: '12px',
    padding: '16px',
    backgroundColor: tokens.colorNeutralBackground1,
    borderRadius: tokens.borderRadiusMedium,
  },
  sectionTitle: {
    fontSize: '18px',
    fontWeight: '600',
    marginBottom: '4px',
  },
  row: {
    display: 'flex',
    flexDirection: 'column',
    gap: '4px',
  },
  buttonRow: {
    display: 'flex',
    gap: '8px',
    marginTop: '8px',
  },
  inlineAuthRow: {
    display: 'flex',
    gap: '8px',
    alignItems: 'flex-end',
  },
})

export default function SettingsPanel() {
  const styles = useStyles()
  const { setThemeMode, addToast } = useAppStore()
  
  // 1. 存储后端类型切换状态：netdisk | alist | webdav | s3
  const [storageType, setStorageType] = useState<'netdisk' | 'alist' | 'webdav' | 's3'>('netdisk')
  const [isConnected, setIsConnected] = useState(false)
  const [testLoading, setTestLoading] = useState(false)
  const [testError, setTestError] = useState<string | null>(null)
  const [backupRoot, setBackupRoot] = useState<string>('')

  // 2. 直连网盘相关配置表单 State (基于 api.oplist.org SaaS 托管免部署)
  const [netdiskDriver, setNetdiskDriver] = useState('baiduyun_go')
  const [netdiskToken, setNetdiskToken] = useState('')
  const [netdiskRefreshToken, setNetdiskRefreshToken] = useState('') // 网盘刷新令牌 (Refresh Token) 状态管理

  // 3. 自建 Alist 驱动相关配置表单 State
  const [alistUrl, setAlistUrl] = useState('')
  const [alistUsername, setAlistUsername] = useState('')
  const [alistPassword, setAlistPassword] = useState('')
  const [alistToken, setAlistToken] = useState('')
  const [alistProvider, setAlistProvider] = useState('alist')

  // 4. WebDAV 驱动相关配置表单 State
  const [webdavEndpoint, setWebdavEndpoint] = useState('')
  const [webdavUsername, setWebdavUsername] = useState('')
  const [webdavPassword, setWebdavPassword] = useState('')

  // 5. S3 驱动相关配置表单 State
  const [s3Endpoint, setS3Endpoint] = useState('')
  const [s3Bucket, setS3Bucket] = useState('')
  const [s3AccessKeyId, setS3AccessKeyId] = useState('')
  const [s3SecretAccessKey, setS3SecretAccessKey] = useState('')
  const [s3Region, setS3Region] = useState('')

  // 6. 应用全局主题与 API Key State
  const [theme, setTheme] = useState('system')
  const [apiKey, setApiKey] = useState('')
  const [saving, setSaving] = useState(false)
  // 夸克 TV 扫码登录状态
  const [tvQrOpen, setTvQrOpen] = useState(false)
  const [tvQrImage, setTvQrImage] = useState('')
  const [tvQrToken, setTvQrToken] = useState('')
  const [tvQrPolling, setTvQrPolling] = useState(false)

  // 初始化拉取全局配置文件并渲染表单
  useEffect(() => {
    // 每次进入设置页，主动静默轮询刷新所有已配置网盘凭证并自动持久化存盘
    storageRefreshAllTokens().then((hasRefreshed) => {
      if (hasRefreshed) {
        console.info('[Netdisk] 全自动静默刷新 Token 成功并已重新存盘。')
        // 如果成功刷新了，重新读取配置以更新最新的令牌状态到表单中
        loadConfig().then((config) => {
          if (config.storage && config.storage.type === 'netdisk') {
            setNetdiskToken(config.storage.token)
            setNetdiskRefreshToken(config.storage.refresh_token ?? '')
          }
        })
      }
    }).catch(err => {
      console.warn('[Netdisk] 轮询刷新 Token 异常:', err)
    })

    loadConfig().then((config) => {
      setTheme(config.settings?.theme ?? 'system')
      setApiKey(config.settings?.steamgriddb_api_key ?? '')

      // 优先解析多后端存储配置
      if (config.storage) {
        setStorageType(config.storage.type)
        setIsConnected(true) // 默认标记为可用连接

        if (config.storage.type === 'netdisk') {
          setNetdiskDriver(config.storage.driver)
          setNetdiskToken(config.storage.token)
          setNetdiskRefreshToken(config.storage.refresh_token ?? '') // 同步读取并回显网盘刷新令牌
          setBackupRoot(config.storage.backup_root ?? '')
        } else if (config.storage.type === 'alist') {
          setAlistUrl(config.storage.base_url)
          setAlistUsername(config.storage.username)
          setAlistToken(config.storage.token ?? '')
          setAlistPassword(config.storage.password ?? '')
          setAlistProvider(config.storage.provider)
          setBackupRoot(config.storage.backup_root ?? '')
        } else if (config.storage.type === 'webdav') {
          setWebdavEndpoint(config.storage.endpoint)
          setWebdavUsername(config.storage.username)
          setWebdavPassword(config.storage.password)
          setBackupRoot(config.storage.backup_root ?? '')
        } else if (config.storage.type === 's3') {
          setS3Endpoint(config.storage.endpoint)
          setS3Bucket(config.storage.bucket)
          setS3AccessKeyId(config.storage.access_key_id)
          setS3SecretAccessKey(config.storage.secret_access_key)
          setS3Region(config.storage.region ?? '')
          setBackupRoot(config.storage.backup_root ?? '')
        }
      } else if (config.alist) {
        // 向下兼容老版本自建 Alist 配置
        setStorageType('alist')
        setIsConnected(!!config.alist.token || !!config.alist.password)
        setAlistUrl(config.alist.base_url)
        setAlistUsername(config.alist.username)
        setAlistToken(config.alist.token ?? '')
        setAlistPassword(config.alist.password ?? '')
        setAlistProvider(config.alist.provider)
      }
    })
  }, [])

  // 构造当前的临时表单配置对象
  const getTempStorageConfig = (): StorageConfig => {
    if (storageType === 'netdisk') {
      return {
        type: 'netdisk',
        driver: netdiskDriver,
        token: netdiskToken,
        refresh_token: netdiskRefreshToken || undefined, // 封装并传递刷新令牌
        backup_root: backupRoot || undefined,
      }
    } else if (storageType === 'alist') {
      return {
        type: 'alist',
        base_url: alistUrl,
        username: alistUsername,
        token: alistToken || undefined,
        password: alistPassword || undefined,
        provider: alistProvider,
        backup_root: backupRoot || undefined,
      }
    } else if (storageType === 'webdav') {
      return {
        type: 'webdav',
        endpoint: webdavEndpoint,
        username: webdavUsername,
        password: webdavPassword,
        backup_root: backupRoot || undefined,
      }
    } else {
      return {
        type: 's3',
        endpoint: s3Endpoint,
        bucket: s3Bucket,
        access_key_id: s3AccessKeyId,
        secret_access_key: s3SecretAccessKey,
        region: s3Region || undefined,
        backup_root: backupRoot || undefined,
      }
    }
  }

  // 测试云端存储连通性与权限
  const handleTestConnection = async () => {
    setTestLoading(true)
    setTestError(null)
    try {
      const tempConfig = getTempStorageConfig()
      const success = await storageTestConnection(tempConfig)
      if (success) {
        setIsConnected(true)
        addToast('云端备份存储连接测试成功！目录可读写。', 'success')
      }
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setTestError(msg)
      addToast(`连接测试失败: ${msg}`, 'error')
      setIsConnected(false)
    } finally {
      setTestLoading(false)
    }
  }

  // 黑科技：在子 Webview 窗口中开启 DOM 轮询注入以自动无感截获网盘 Token
  const handleOplistAuth = async () => {
    try {
      addToast('正在拉取云端免配置网关，请在弹出的小窗口中扫码或跳转授权...', 'info')
      
      // 1. 动态加载 Tauri Webview API 并生成高优先级前端悬浮小窗口
      const { WebviewWindow } = await import('@tauri-apps/api/webviewWindow')
      const authWindow = new WebviewWindow('oplist-auth-helper', {
        url: 'https://api.oplist.org/',
        title: '🔐 网盘一键扫码云授权',
        width: 480,
        height: 640,
        resizable: false,
        alwaysOnTop: true,
      })

      authWindow.once('tauri://destroyed', () => {
        addToast('网盘授权窗口已关闭', 'info')
      })

      // 2. 使用定时器进行微型 DOM 轮询 eval 注入，捕获并自动控制网页元素实现无感极简授权
      const checkInterval = setInterval(async () => {
        try {
          const captured = await (authWindow as any).eval(`
            (() => {
              // --- 1. 自动在网盘下拉框中选中与父窗口匹配的云盘类型 ---
              const selectEl = document.querySelector('select');
              if (selectEl && !selectEl.dataset.autoSelected) {
                const driver = "${netdiskDriver}"; // 获取当前组件的网盘驱动状态
                let matchedIndex = -1;
                for (let i = 0; i < selectEl.options.length; i++) {
                  const optText = selectEl.options[i].text.toLowerCase();
                  const optVal = selectEl.options[i].value.toLowerCase();
                  
                  let isMatch = false;
                  // 根据驱动前缀名称进行中英文自适应模糊匹配
                  if (driver.includes('alicloud') && (optText.includes('阿里云') || optText.includes('alicloud') || optText.includes('aliyun') || optVal.includes('alicloud') || optVal.includes('aliyun'))) {
                    isMatch = true;
                  } else if (driver.includes('baidu') && (optText.includes('百度') || optText.includes('baidu') || optVal.includes('baidu'))) {
                    isMatch = true;
                  } else if (driver.includes('onedrive') && (optText.includes('onedrive') || optVal.includes('onedrive'))) {
                    isMatch = true;
                  } else if (driver.includes('quark') && (optText.includes('夸克') || optText.includes('quark') || optVal.includes('quark'))) {
                    isMatch = true;
                  }
                  
                  if (isMatch) {
                    matchedIndex = i;
                    break;
                  }
                }
                
                if (matchedIndex !== -1) {
                  selectEl.selectedIndex = matchedIndex;
                  // 发送原生 change 事件以确保前端响应式框架能够成功更新 State
                  selectEl.dispatchEvent(new Event('change', { bubbles: true }));
                  selectEl.dataset.autoSelected = 'true';
                }
              }

              // --- 2. 自动勾选使用 OpenList 提供的授权密钥参数 ---
              const checkboxEl = document.querySelector('input[type="checkbox"]');
              if (checkboxEl && !checkboxEl.checked && !checkboxEl.dataset.autoChecked) {
                checkboxEl.checked = true;
                checkboxEl.dispatchEvent(new Event('change', { bubbles: true }));
                checkboxEl.dispatchEvent(new Event('click', { bubbles: true }));
                checkboxEl.dataset.autoChecked = 'true';
              }

              // --- 3. 自动匹配并点击网页上的“获取 Token”按钮进入登录扫码 ---
              // 核心安全防护锁：只有当 access-token 文本框尚未生成有效 Token 时才允许自动触发点击。
              // 彻底防止扫码/授权重定向回到本页产生 Token 的瞬间，被轮询脚本再次无限重复点击并清空冲刷 Token！
              const tokenCheckEl = document.getElementById('access-token');
              const hasToken = tokenCheckEl && tokenCheckEl.value && tokenCheckEl.value.trim().length > 15;

              if (!hasToken) {
                const buttons = document.querySelectorAll('button');
                for (let btn of buttons) {
                  const btnText = btn.textContent || btn.innerText;
                  if (btnText && (btnText.includes('获取 Token') || btnText.includes('获取Token')) && !btn.dataset.autoClicked) {
                    btn.click();
                    btn.dataset.autoClicked = 'true';
                    break;
                  }
                }
              }

              // --- 4. 常规轮询捕获 Access/Refresh Token ---
              const tokenEl = document.getElementById('access-token');
              const refreshEl = document.getElementById('refresh-token');
              if (tokenEl && tokenEl.value && tokenEl.value.trim().length > 15) {
                return {
                  accessToken: tokenEl.value.trim(),
                  refreshToken: refreshEl ? refreshEl.value.trim() : ''
                };
              }
              return null;
            })()
          `)

          if (captured) {
            clearInterval(checkInterval)
            setNetdiskToken(captured.accessToken)
            setNetdiskRefreshToken(captured.refreshToken)
            setIsConnected(true)
            addToast('一键网盘授权捕获成功！访问令牌与刷新令牌已自动锁定回填！', 'success')
            authWindow.destroy()
          }
        } catch (e) {
          // 忽略在页面跳转期 DOM 注入失败的报错
        }
      }, 1000)

      // 10 分钟防死锁安全熔断
      setTimeout(() => {
        clearInterval(checkInterval)
      }, 600000)

    } catch (err) {
      addToast('拉取网盘授权窗口失败，请检查网络或配置', 'error')
    }
  }
  // 夸克 TV 扫码登录完整流程
  const handleQuarkTvScan = async () => {
    try {
      setTvQrOpen(true)
      setTvQrPolling(false)
      const [qrData, queryToken] = await quarkTvGetQrCode()
      setTvQrImage(`data:image/png;base64,${qrData}`)
      setTvQrToken(queryToken)
      
      // 开始轮询
      setTvQrPolling(true)
      const pollInterval = setInterval(async () => {
        try {
          const code = await quarkTvPollQr(queryToken)
          if (code) {
            clearInterval(pollInterval)
            setTvQrPolling(false)
            const [accessToken, refreshToken] = await quarkTvExchange(code)
            setNetdiskToken(accessToken)
            setNetdiskRefreshToken(refreshToken)
            setIsConnected(true)
            setTvQrOpen(false)
            addToast('夸克 TV 扫码登录成功！令牌已自动填充。', 'success')
          }
        } catch (e) {
          clearInterval(pollInterval)
          setTvQrPolling(false)
          addToast(`扫码轮询异常: ${e}`, 'error')
        }
      }, 2000)

      // 2 分钟超时自动停止
      setTimeout(() => {
        clearInterval(pollInterval)
        if (tvQrPolling) {
          setTvQrPolling(false)
          addToast('扫码登录超时，请重试', 'warning')
        }
      }, 120000)
    } catch (err) {
      setTvQrOpen(false)
      addToast(`获取二维码失败: ${err}`, 'error')
    }
  }

  // 针对自建 Alist 专属的登录换取 Token 操作
  const handleAlistLogin = async () => {
    if (!alistUrl || !alistUsername || !alistPassword) {
      addToast('请输入自建 Alist 服务器、用户名和密码以登录换取 Token', 'warning')
      return
    }
    setTestLoading(true)
    setTestError(null)
    try {
      const res = await alistLogin({ url: alistUrl, username: alistUsername, password: alistPassword })
      setAlistToken(res.token)
      setIsConnected(true)
      addToast('自建 Alist 登录认证成功！授权令牌已自动捕获刷新。', 'success')
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err)
      setTestError(msg)
      addToast(`自建端登录授权失败: ${msg}`, 'error')
      setIsConnected(false)
    } finally {
      setTestLoading(false)
    }
  }

  // 断开连接，切换测试状态
  const handleDisconnect = () => {
    setIsConnected(false)
    addToast('已断开云盘浏览状态，如需重新浏览请点击连接测试', 'info')
  }

  // 保存整个设置面板的配置项到磁盘中
  const handleSave = async () => {
    setSaving(true)
    try {
      const config = await loadConfig()
      config.settings.theme = theme
      config.settings.steamgriddb_api_key = apiKey || undefined

      // 组装并写入多存储后端激活参数
      if (storageType === 'netdisk') {
        config.storage = {
          type: 'netdisk',
          driver: netdiskDriver,
          token: netdiskToken,
          refresh_token: netdiskRefreshToken || undefined, // 保存时携带刷新令牌
          backup_root: backupRoot || undefined,
        }
      } else if (storageType === 'alist') {
        config.storage = {
          type: 'alist',
          base_url: alistUrl,
          username: alistUsername,
          token: alistToken || undefined,
          password: alistPassword || undefined,
          provider: alistProvider,
          backup_root: backupRoot || undefined,
        }
      } else if (storageType === 'webdav') {
        config.storage = {
          type: 'webdav',
          endpoint: webdavEndpoint,
          username: webdavUsername,
          password: webdavPassword,
          backup_root: backupRoot || undefined,
        }
      } else if (storageType === 's3') {
        config.storage = {
          type: 's3',
          endpoint: s3Endpoint,
          bucket: s3Bucket,
          access_key_id: s3AccessKeyId,
          secret_access_key: s3SecretAccessKey,
          region: s3Region || undefined,
          backup_root: backupRoot || undefined,
        }
      }

      await saveConfig(config)
      addToast('全局设置与云盘配置已成功保存！', 'success')
    } catch (err) {
      addToast(err instanceof Error ? err.message : '保存失败', 'error')
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className={styles.root}>
      {/* 统一存储连接配置 */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>云端备份配置</div>
        
        {/* 选择存储介质类型 */}
        <div className={styles.row}>
          <Label style={{ marginBottom: '4px' }}>存储同步类型</Label>
          <RadioGroup
            layout="vertical"
            value={storageType}
            onChange={(_, data) => {
              setStorageType(data.value as 'netdisk' | 'alist' | 'webdav' | 's3')
              setIsConnected(false) // 切换存储类型时需重新测试连接
              setTestError(null)
            }}
          >
            <Radio value="netdisk" label="直连网盘 (SaaS 免部署 - 百度网盘 / OneDrive / 阿里云盘 / 夸克)" />
            <Radio value="alist" label="自建 Alist 服务 (折腾折腾 - 私有化本地部署或自建服务器)" />
            <Radio value="webdav" label="标准 WebDAV 协议 (如坚果云、Nextcloud、私有 NAS 挂载)" />
            <Radio value="s3" label="标准对象存储 S3 (即将推出)" disabled />
          </RadioGroup>
        </div>

        {isConnected ? (
          <>
            <MessageBar intent="success">
              <MessageBarBody>
                已成功锁定云端存储连接 ({storageType === 'netdisk' ? '网盘直连模式' : storageType === 'alist' ? alistUrl : storageType === 'webdav' ? webdavEndpoint : s3Bucket})
              </MessageBarBody>
            </MessageBar>
            <div className={styles.buttonRow}>
              <Button onClick={handleDisconnect}>断开连接</Button>
            </div>
          </>
        ) : (
          <>
            {testError && (
              <MessageBar intent="error">
                <MessageBarBody>{testError}</MessageBarBody>
              </MessageBar>
            )}

            {/* 1. 直连网盘表单区域（全新第一等公民支持） */}
            {storageType === 'netdisk' && (
              <>
                <div className={styles.row}>
                  <Label htmlFor="netdiskDriver">选择网盘类型</Label>
                  <RadioGroup
                    layout="horizontal"
                    value={netdiskDriver}
                    onChange={(_, data) => {
                      setNetdiskDriver(data.value)
                      setIsConnected(false)
                    }}
                  >
                    <Radio value="baiduyun_go" label="百度网盘" />
                    <Radio value="onedrive_go" label="OneDrive" />
                    <Radio value="alicloud_qr" label="阿里云盘" />
                    <Radio value="quark_uc" label="夸克网盘 (Cookie)" />
                    <Radio value="quark_uc_tv" label="夸克TV (扫码登录)" />
                  </RadioGroup>
                </div>
                {/* 根据网盘类型动态显示不同的认证字段 */}
                {netdiskDriver === 'quark_uc' ? (
                  <div className={styles.row}>
                    <Label htmlFor="netdiskToken">Cookie 字符串</Label>
                    <Input
                      id="netdiskToken"
                      value={netdiskToken}
                      onChange={(e) => setNetdiskToken(e.target.value)}
                      placeholder="从浏览器 F12 → Application → Cookies 复制完整 Cookie 字符串"
                    />
                  </div>
                ) : (
                  <>
                    <div className={styles.row}>
                      <Label htmlFor="netdiskToken">网盘授权令牌 (Access Token)</Label>
                      <div className={styles.inlineAuthRow}>
                        <Input
                          id="netdiskToken"
                          value={netdiskToken}
                          onChange={(e) => setNetdiskToken(e.target.value)}
                          placeholder="点击右侧一键授权，授权完成后 Token 会自动抓取到这里"
                          style={{ flexGrow: 1 }}
                        />
                        <Button appearance="primary" onClick={
                          netdiskDriver === 'quark_uc_tv' ? handleQuarkTvScan : handleOplistAuth
                        }>
                          {netdiskDriver === 'quark_uc_tv' ? '📱 扫码登录' : '🔐 一键免配置授权'}
                        </Button>
                      </div>
                    </div>
                    <div className={styles.row}>
                      <Label htmlFor="netdiskRefreshToken">网盘刷新令牌 (Refresh Token)</Label>
                      <Input
                        id="netdiskRefreshToken"
                        value={netdiskRefreshToken}
                        onChange={(e) => setNetdiskRefreshToken(e.target.value)}
                        placeholder="授权完成后，Refresh Token 会自动抓取并填充至此处 (如支持)"
                      />
                    </div>
              </>
            )}
              </>
            )}
            {/* 夸克 TV 扫码弹窗 */}
            {tvQrOpen && (
              <div style={{
                position: 'fixed', top: 0, left: 0, right: 0, bottom: 0,
                backgroundColor: 'rgba(0,0,0,0.5)', display: 'flex',
                alignItems: 'center', justifyContent: 'center', zIndex: 1000,
              }}>
                <div style={{
                  backgroundColor: tokens.colorNeutralBackground1,
                  borderRadius: '12px', padding: '24px', textAlign: 'center',
                  maxWidth: '380px',
                }}>
                  <h3 style={{ margin: '0 0 12px' }}>夸克 TV 扫码登录</h3>
                  {tvQrImage ? (
                    <img src={tvQrImage} alt="QR Code" style={{ width: '280px', height: '280px' }} />
                  ) : (
                    <Spinner label="加载二维码中..." />
                  )}
                  <p style={{ color: tokens.colorNeutralForeground3, fontSize: '13px', margin: '12px 0' }}>
                    {tvQrPolling ? '请使用夸克 App 扫描二维码授权' : '正在等待授权...'}
                  </p>
                  <Button onClick={() => setTvQrOpen(false)}>取消</Button>
                </div>
              </div>
            )}

            {/* 2. 自建 Alist 表单区域 */}
            {storageType === 'alist' && (
              <>
                <div className={styles.row}>
                  <Label htmlFor="alistUrl">Alist 服务器地址</Label>
                  <Input
                    id="alistUrl"
                    value={alistUrl}
                    onChange={(e) => setAlistUrl(e.target.value)}
                    placeholder="例如 http://192.168.0.21:5244"
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="alistUsername">用户名</Label>
                  <Input
                    id="alistUsername"
                    value={alistUsername}
                    onChange={(e) => setAlistUsername(e.target.value)}
                    placeholder="请输入自建 Alist 用户名"
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="alistPassword">自建端密码</Label>
                  <div className={styles.inlineAuthRow}>
                    <Input
                      id="alistPassword"
                      type="password"
                      value={alistPassword}
                      onChange={(e) => setAlistPassword(e.target.value)}
                      placeholder="自建端密码 (填入密码后，后端可免 Token 全自动登录并保存密码)"
                      style={{ flexGrow: 1 }}
                    />
                    <Button onClick={handleAlistLogin} disabled={testLoading}>
                      登录获取 Token
                    </Button>
                  </div>
                </div>
                <div className={styles.row}>
                  <Label htmlFor="alistToken">授权 Token (令牌)</Label>
                  <Input
                    id="alistToken"
                    value={alistToken}
                    onChange={(e) => setAlistToken(e.target.value)}
                    placeholder="授权 Token (可选，如填入密码，本行可留空)"
                  />
                </div>
              </>
            )}

            {/* 3. WebDAV 表单区域 */}
            {storageType === 'webdav' && (
              <>
                <div className={styles.row}>
                  <Label htmlFor="webdavEndpoint">WebDAV 服务地址 (Endpoint)</Label>
                  <Input
                    id="webdavEndpoint"
                    value={webdavEndpoint}
                    onChange={(e) => setWebdavEndpoint(e.target.value)}
                    placeholder="例如 https://dav.jianguoyun.com/dav/"
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="webdavUsername">WebDAV 账户名</Label>
                  <Input
                    id="webdavUsername"
                    value={webdavUsername}
                    onChange={(e) => setWebdavUsername(e.target.value)}
                    placeholder="请输入您的 WebDAV 账户名"
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="webdavPassword">WebDAV 独立应用口令/密码</Label>
                  <Input
                    id="webdavPassword"
                    type="password"
                    value={webdavPassword}
                    onChange={(e) => setWebdavPassword(e.target.value)}
                    placeholder="请输入第三方应用独立口令"
                  />
                </div>
              </>
            )}

            {/* 4. S3 表单区域 */}
            {storageType === 's3' && (
              <>
                <div className={styles.row}>
                  <MessageBar intent="warning">
                    <MessageBarBody>S3 对象存储驱动目前处于实验性阶段，上传下载处于 Beta 周期。</MessageBarBody>
                  </MessageBar>
                </div>
                <div className={styles.row}>
                  <Label htmlFor="s3Endpoint">S3 物理端点地址 (Endpoint)</Label>
                  <Input
                    id="s3Endpoint"
                    value={s3Endpoint}
                    onChange={(e) => setS3Endpoint(e.target.value)}
                    placeholder="例如 https://s3.us-east-1.amazonaws.com"
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="s3Bucket">物理桶名称 (Bucket)</Label>
                  <Input
                    id="s3Bucket"
                    value={s3Bucket}
                    onChange={(e) => setS3Bucket(e.target.value)}
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="s3AccessKeyId">Access Key ID</Label>
                  <Input
                    id="s3AccessKeyId"
                    value={s3AccessKeyId}
                    onChange={(e) => setS3AccessKeyId(e.target.value)}
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="s3SecretAccessKey">Secret Access Key</Label>
                  <Input
                    id="s3SecretAccessKey"
                    type="password"
                    value={s3SecretAccessKey}
                    onChange={(e) => setS3SecretAccessKey(e.target.value)}
                  />
                </div>
                <div className={styles.row}>
                  <Label htmlFor="s3Region">存储区域 (Region)</Label>
                  <Input
                    id="s3Region"
                    value={s3Region}
                    onChange={(e) => setS3Region(e.target.value)}
                    placeholder="可选，默认 us-east-1"
                  />
                </div>
              </>
            )}

            <div className={styles.buttonRow}>
              <Button appearance="primary" onClick={handleTestConnection} disabled={testLoading}>
                {testLoading ? <Spinner size="tiny" /> : '连接并测试'}
              </Button>
            </div>
          </>
        )}
      </div>

      {/* 云端目录物理浏览器（仅在连通测试成功后展出，实现安全前置） */}
      {isConnected && (
        <div className={styles.section}>
          <div className={styles.sectionTitle}>云端备份目录锁</div>
          <StorageBrowser tempConfig={getTempStorageConfig()} onBackupRootChange={setBackupRoot} />
        </div>
      )}

      {/* 应用全局主题与 API Key 设置 */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>全局设置</div>
        <div className={styles.row}>
          <Label>主题</Label>
          <RadioGroup
            layout="horizontal"
            value={theme}
            onChange={(_, data) => {
              setTheme(data.value)
              setThemeMode(data.value as 'system' | 'light' | 'dark')
            }}
          >
            <Radio value="system" label="跟随系统" />
            <Radio value="light" label="浅色" />
            <Radio value="dark" label="深色" />
          </RadioGroup>
        </div>
        <div className={styles.row}>
          <Label htmlFor="apikey">SteamGridDB API Key</Label>
          <Input
            id="apikey"
            value={apiKey}
            onChange={(e) => setApiKey(e.target.value)}
            placeholder="可选，用于获取游戏封面"
          />
        </div>
        <div className={styles.buttonRow}>
          <Button appearance="primary" onClick={handleSave} disabled={saving}>
            {saving ? <Spinner size="tiny" /> : '保存设置'}
          </Button>
        </div>
      </div>
    </div>
  )
}
