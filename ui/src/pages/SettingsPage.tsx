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
})

export default function SettingsPage() {
  const styles = useStyles()

  return (
    <div className={styles.root}>
      <Title1 style={{ marginBottom: '20px' }}>设置</Title1>
      <SettingsPanel />
    </div>
  )
}
