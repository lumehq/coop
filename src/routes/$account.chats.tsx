import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/$account/chats')({
  component: () => <div>Hello /$account/chats!</div>
})