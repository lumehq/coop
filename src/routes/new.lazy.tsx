import { createLazyFileRoute } from '@tanstack/react-router'

export const Route = createLazyFileRoute('/new')({
  component: () => <div>Hello /new!</div>
})