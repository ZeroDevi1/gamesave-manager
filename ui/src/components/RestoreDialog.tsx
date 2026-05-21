// components/RestoreDialog.tsx - 选择备份版本后恢复
import {
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Button,
  RadioGroup,
  Radio,
  Spinner,
  Text,
  Badge,
  tokens,
} from '@fluentui/react-components'
import { Clock24Regular, Cloud24Regular } from '@fluentui/react-icons'
import { useState, useMemo } from 'react'
import type { BackupManifest, RemoteBackupEntry } from '../services/tauri'

interface RestoreDialogProps {
  open: boolean
  gameName: string
  localHistory: BackupManifest[]
  remoteBackups: RemoteBackupEntry[]
  onClose: () => void
  onRestoreLocal: (timestamp: string) => Promise<void>
  onRestoreRemote: (remotePath: string) => Promise<void>
}

export default function RestoreDialog({
  open,
  gameName,
  localHistory,
  remoteBackups,
  onClose,
  onRestoreLocal,
  onRestoreRemote,
}: RestoreDialogProps) {
  const [selected, setSelected] = useState<string>('')
  const [loading, setLoading] = useState(false)

  const options = useMemo(() => {
    const list: {
      key: string
      label: string
      type: 'local' | 'remote'
      value: string
      extra?: string
    }[] = []
    localHistory.forEach((h) => {
      list.push({
        key: `local_${h.timestamp}`,
        label: new Date(h.timestamp).toLocaleString('zh-CN'),
        type: 'local',
        value: h.timestamp,
        extra: h.backup_type === 'full' ? '全量' : '增量',
      })
    })
    remoteBackups.forEach((rb) => {
      list.push({
        key: `remote_${rb.path}`,
        label: rb.name,
        type: 'remote',
        value: rb.path,
        extra: rb.modified
          ? new Date(rb.modified).toLocaleString('zh-CN')
          : undefined,
      })
    })
    return list
  }, [localHistory, remoteBackups])

  const handleConfirm = async () => {
    if (!selected) return
    const opt = options.find((o) => o.key === selected)
    if (!opt) return

    setLoading(true)
    try {
      if (opt.type === 'local') {
        await onRestoreLocal(opt.value)
      } else {
        await onRestoreRemote(opt.value)
      }
    } finally {
      setLoading(false)
      onClose()
      setSelected('')
    }
  }

  const totalCount = localHistory.length + remoteBackups.length

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface>
        <DialogBody>
          <DialogTitle>恢复存档 — {gameName}</DialogTitle>
          <DialogContent>
            {totalCount === 0 ? (
              <Text style={{ color: tokens.colorNeutralForeground3 }}>
                暂无可用备份，请先执行备份操作。
              </Text>
            ) : (
              <RadioGroup
                value={selected}
                onChange={(_, data) => setSelected(data.value)}
                style={{ display: 'flex', flexDirection: 'column', gap: '8px' }}
              >
                {options.map((opt) => (
                  <div
                    key={opt.key}
                    style={{
                      display: 'flex',
                      alignItems: 'center',
                      gap: '8px',
                      padding: '4px 0',
                    }}
                  >
                    <Radio value={opt.key} />
                    {opt.type === 'local' ? (
                      <Clock24Regular
                        style={{ color: tokens.colorBrandForeground1 }}
                      />
                    ) : (
                      <Cloud24Regular
                        style={{ color: tokens.colorBrandForeground1 }}
                      />
                    )}
                    <div style={{ flexGrow: 1, minWidth: 0 }}>
                      <div
                        style={{
                          fontWeight: 600,
                          overflow: 'hidden',
                          textOverflow: 'ellipsis',
                          whiteSpace: 'nowrap',
                        }}
                        title={opt.label}
                      >
                        {opt.label}
                      </div>
                      {opt.extra && (
                        <div style={{ fontSize: '12px', color: tokens.colorNeutralForeground3 }}>
                          {opt.extra}
                        </div>
                      )}
                    </div>
                    {opt.type === 'local' && (
                      <Badge appearance="filled" size="small">
                        {opt.extra}
                      </Badge>
                    )}
                    {opt.type === 'remote' && (
                      <Badge appearance="outline" size="small">
                        远程
                      </Badge>
                    )}
                  </div>
                ))}
              </RadioGroup>
            )}
            {loading && (
              <div style={{ marginTop: '16px' }}>
                <Spinner label="正在恢复..." />
              </div>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onClose} disabled={loading}>
              取消
            </Button>
            <Button
              appearance="primary"
              onClick={handleConfirm}
              disabled={loading || !selected || totalCount === 0}
            >
              恢复选中版本
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  )
}
