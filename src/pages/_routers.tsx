import { createBrowserRouter, RouteObject } from 'react-router'

import Layout from './_layout'
import { navItems } from './_nav-items'

export { navItems } from './_nav-items'

export const router = createBrowserRouter([
  {
    path: '/',
    Component: Layout,
    children: navItems.map(
      (item) =>
        ({
          path: item.path,
          Component: item.Component,
        }) as RouteObject,
    ),
  },
])
