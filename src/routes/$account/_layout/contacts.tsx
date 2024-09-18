import { commands } from '@/commands'
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/$account/_layout/contacts')({
  loader: async () => {
    const res = await commands.getContactList()

    if (res.status === 'ok') {
      return res.data
    } else {
      return []
    }
  },
})
