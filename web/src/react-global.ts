// The Flint bundle reads `window.React` when it loads; this must be imported before it.
import React from 'react'

;(window as unknown as { React: typeof React }).React = React
