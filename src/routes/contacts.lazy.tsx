import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/contacts')({
  component: () => <div>Hello /contacts!</div>
})