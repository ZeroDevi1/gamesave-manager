// components/ToastContainer.tsx - 全局 Toast 容器
import {
  Toaster,
  Toast,
  ToastTitle,
  ToastBody,
  useToastController,
} from '@fluentui/react-components'
import { useEffect } from 'react'
import { useAppStore } from '../store/appStore'

export default function ToastContainer() {
  const { toasts, removeToast } = useAppStore()
  const { dispatchToast } = useToastController('global-toaster')

  useEffect(() => {
    toasts.forEach((t) => {
      dispatchToast(
        <Toast>
          <ToastTitle>{getTitle(t.intent)}</ToastTitle>
          <ToastBody>{t.message}</ToastBody>
        </Toast>,
        { intent: t.intent, timeout: 4000 },
      )
      removeToast(t.id)
    })
  }, [toasts, dispatchToast, removeToast])

  return <Toaster toasterId="global-toaster" />
}

function getTitle(intent: string): string {
  switch (intent) {
    case 'success': return '成功'
    case 'error': return '错误'
    case 'warning': return '警告'
    default: return '提示'
  }
}
