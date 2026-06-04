// components/BackupDialog.tsx - 备份确认弹窗（选择策略）
import {
  Dialog,
  DialogSurface,
  DialogTitle,
  DialogBody,
  DialogActions,
  DialogContent,
  Button,
  Label,
  RadioGroup,
  Radio,
  Spinner,
} from '@fluentui/react-components'
import { useState } from 'react'

interface BackupDialogProps {
  open: boolean
  gameName: string
  onClose: () => void
  onConfirm: (type: 'full' | 'incremental') => Promise<void>
}

export default function BackupDialog({ open, gameName, onClose, onConfirm }: BackupDialogProps) {
  const [type, setType] = useState<'full' | 'incremental'>('full')
  const [loading, setLoading] = useState(false)

  const handleConfirm = async () => {
    setLoading(true)
    try {
      await onConfirm(type)
    } finally {
      setLoading(false)
      onClose()
    }
  }

  return (
    <Dialog open={open} onOpenChange={(_, data) => !data.open && onClose()}>
      <DialogSurface
        style={{
          backgroundColor: 'var(--lg-bg-elevated)',
          backdropFilter: 'blur(var(--lg-blur-lg)) saturate(var(--lg-saturate))',
          WebkitBackdropFilter: 'blur(var(--lg-blur-lg)) saturate(var(--lg-saturate))',
          border: '1px solid var(--lg-border-color)',
          borderRadius: 'var(--lg-radius-xl)',
          boxShadow: 'var(--lg-shadow-high)',
        }}
      >
        <DialogBody>
          <DialogTitle style={{ color: 'var(--lg-text-primary)' }}>备份确认 — {gameName}</DialogTitle>
          <DialogContent style={{ color: 'var(--lg-text-primary)' }}>
            <Label>选择备份策略</Label>
            <RadioGroup
              value={type}
              onChange={(_, data) => setType(data.value as 'full' | 'incremental')}
            >
              <Radio value="full" label="全量备份 — 打包所有存档文件并上传" />
              <Radio value="incremental" label="增量备份 — 仅上传变更的文件" />
            </RadioGroup>
            {loading && (
              <div style={{ marginTop: '16px' }}>
                <Spinner label="正在备份..." />
              </div>
            )}
          </DialogContent>
          <DialogActions>
            <Button appearance="secondary" onClick={onClose} disabled={loading}>取消</Button>
            <Button appearance="primary" onClick={handleConfirm} disabled={loading}>
              开始备份
            </Button>
          </DialogActions>
        </DialogBody>
      </DialogSurface>
    </Dialog>
  )
}
