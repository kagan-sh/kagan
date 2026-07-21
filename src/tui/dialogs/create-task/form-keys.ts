type DescriptionField = { plainText?: string; newLine?: () => void }

export function handleCreateTaskKey(props: {
  key: { name: string; ctrl?: boolean; shift?: boolean }
  focusIndex: () => number
  setFocusIndex: (value: number | ((index: number) => number)) => void
  descriptionRef: DescriptionField | undefined
  submit: () => void
  openPicker: () => void
}): boolean {
  const { key } = props
  if (key.ctrl && key.name === "return") {
    props.submit()
    return true
  }
  if (
    props.focusIndex() === 1 &&
    ((key.ctrl && key.name === "j") || key.name === "linefeed" || (key.shift && key.name === "return"))
  ) {
    props.descriptionRef?.newLine?.()
    return true
  }
  if (key.name === "return") {
    if (props.focusIndex() >= 2) props.openPicker()
    else props.submit()
    return true
  }
  if (key.name === "right" && props.focusIndex() >= 2) {
    props.openPicker()
    return true
  }
  if (key.name === "tab") {
    props.setFocusIndex((index) => (key.shift ? (index + 4) % 5 : (index + 1) % 5))
    return true
  }
  if (props.focusIndex() === 1) return false
  if (key.name === "down") {
    props.setFocusIndex((index) => (index + 1) % 5)
    return true
  }
  if (key.name === "up") {
    props.setFocusIndex((index) => (index + 4) % 5)
    return true
  }
  return false
}
