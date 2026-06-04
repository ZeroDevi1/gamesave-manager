// pages/SettingsPage.tsx - 全局设置页
import { makeStyles, Title1 } from '@fluentui/react-components'
import SettingsPanel from '../components/SettingsPanel'

const useStyles = makeStyles({
  root: {
    padding: '24px',
    maxWidth: '800px',
    height: '100%',
    overflowY: 'auto',
    scrollbarWidth: 'none',
    '::-webkit-scrollbar': {
      display: 'none',
    },
  },
  title: {
    marginBottom: '20px',
    color: 'var(--lg-text-primary)',
  },
})

export default function SettingsPage() {
  const styles = useStyles()

  return (
    <div className={styles.root}>
      <Title1 className={styles.title}>设置</Title1>
      <SettingsPanel />
    </div>
  )
}
