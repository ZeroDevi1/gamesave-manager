// components/SettingsPanel.tsx - 设置面板（内嵌在设置页）
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
import { loadConfig, saveConfig } from '../services/tauri'
import { useAppStore } from '../store/appStore'
import { useAlist } from '../hooks/useAlist'
import AlistBrowser from './AlistBrowser'

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
})

export default function SettingsPanel() {
  const styles = useStyles()
  const { setThemeMode } = useAppStore()
  const { connected, login, disconnect, loading: alistLoading, error: alistError } = useAlist()
  const [url, setUrl] = useState('')
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [theme, setTheme] = useState('system')
  const [apiKey, setApiKey] = useState('')
  const [saving, setSaving] = useState(false)

  useEffect(() => {
    loadConfig().then((config) => {
      setTheme(config.settings?.theme ?? 'system')
      setApiKey(config.settings?.steamgriddb_api_key ?? '')
      if (config.alist) {
        setUrl(config.alist.base_url)
        setUsername(config.alist.username)
      }
    })
  }, [])

  const handleSave = async () => {
    setSaving(true)
    try {
      const config = await loadConfig()
      config.settings.theme = theme
      config.settings.steamgriddb_api_key = apiKey || undefined
      await saveConfig(config)
    } finally {
      setSaving(false)
    }
  }

  const handleLogin = async () => {
    if (!url || !username || !password) return
    await login(url, username, password)
  }

  return (
    <div className={styles.root}>
      {/* Alist 连接设置 */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>Alist 连接</div>
        {connected ? (
          <>
            <MessageBar intent="success">
              <MessageBarBody>已连接到 {url}</MessageBarBody>
            </MessageBar>
            <Button onClick={disconnect}>断开连接</Button>
          </>
        ) : (
          <>
            {alistError && (
              <MessageBar intent="error">
                <MessageBarBody>{alistError}</MessageBarBody>
              </MessageBar>
            )}
            <div className={styles.row}>
              <Label htmlFor="url">服务器地址</Label>
              <Input
                id="url"
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder="https://alist.example.com"
              />
            </div>
            <div className={styles.row}>
              <Label htmlFor="username">用户名</Label>
              <Input
                id="username"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
              />
            </div>
            <div className={styles.row}>
              <Label htmlFor="password">密码</Label>
              <Input
                id="password"
                type="password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            </div>
            <div className={styles.buttonRow}>
              <Button appearance="primary" onClick={handleLogin} disabled={alistLoading}>
                {alistLoading ? <Spinner size="tiny" /> : '登录'}
              </Button>
            </div>
          </>
        )}
      </div>

      {/* 目录浏览器 */}
      {connected && (
        <div className={styles.section}>
          <div className={styles.sectionTitle}>目录浏览</div>
          <AlistBrowser />
        </div>
      )}

      {/* 全局设置 */}
      <div className={styles.section}>
        <div className={styles.sectionTitle}>全局设置</div>
        <div className={styles.row}>
          <Label>主题</Label>
          <RadioGroup
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
