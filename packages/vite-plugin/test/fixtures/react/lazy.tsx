document.querySelector('#load')!.addEventListener('click', async () => {
  const { html, styles } = await import('./app.js')
  document.querySelector('main')!.innerHTML = html
  const base = document.createElement('button')
  base.id = 'base'
  base.className = styles.override
  document.querySelector('main')!.append(base)
})
