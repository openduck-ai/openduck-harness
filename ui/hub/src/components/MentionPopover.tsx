import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
} from 'react'
import {
  Sparkles,
  ScrollText,
  Folder,
  FileCode,
  FileText,
  File,
  Paperclip,
  Loader2,
  CornerDownLeft,
} from 'lucide-react'
import {
  filterAndRankMentionItems,
  type MentionDisplayItem,
  type MentionItemType,
  type MentionMatchItem,
} from '../mention.ts'

export interface MentionPopoverProps {
  isOpen: boolean
  onClose: () => void
  onSelect: (item: MentionDisplayItem) => void
  position: { x: number; y: number }
  query: string
  selectedIndex: number
  onSelectedIndexChange: (index: number) => void
  items: MentionDisplayItem[]
  loading?: boolean
}

export interface MentionPopoverHandle {
  getDisplayItems: () => MentionMatchItem[]
  selectItem: (index: number) => void
}

function renderItemIcon(item: MentionDisplayItem) {
  switch (item.itemType) {
    case 'Attachment':
      return <Paperclip size={14} className="mention-type-icon attachment" />
    case 'Agent':
      return <Sparkles size={14} className="mention-type-icon agent" />
    case 'Rule':
      return <ScrollText size={14} className="mention-type-icon rule" />
    case 'Directory':
      return <Folder size={14} className="mention-type-icon dir" />
    case 'File': {
      const ext = item.name.split('.').pop()?.toLowerCase()
      if (ext === 'md' || ext === 'txt' || ext === 'doc' || ext === 'pdf') {
        return <FileText size={14} className="mention-type-icon file-doc" />
      }
      if (
        [
          'ts',
          'tsx',
          'js',
          'jsx',
          'rs',
          'py',
          'go',
          'json',
          'toml',
          'yaml',
          'yml',
          'css',
          'html',
          'sh',
          'sql',
        ].includes(ext || '')
      ) {
        return <FileCode size={14} className="mention-type-icon file-code" />
      }
      return <File size={14} className="mention-type-icon file" />
    }
  }
}

function renderHighlightedName(name: string, matchIndices: number[]) {
  if (!matchIndices || matchIndices.length === 0) {
    return <span>{name}</span>
  }

  const matchSet = new Set(matchIndices)
  const elements: React.ReactNode[] = []

  let currentChunk = ''
  let isCurrentMatch = false

  for (let i = 0; i < name.length; i++) {
    const isMatch = matchSet.has(i)
    if (i === 0) {
      isCurrentMatch = isMatch
      currentChunk += name[i]
    } else if (isMatch === isCurrentMatch) {
      currentChunk += name[i]
    } else {
      elements.push(
        isCurrentMatch ? (
          <span key={i} className="mention-char-match">
            {currentChunk}
          </span>
        ) : (
          <span key={i}>{currentChunk}</span>
        ),
      )
      isCurrentMatch = isMatch
      currentChunk = name[i]
    }
  }

  if (currentChunk) {
    elements.push(
      isCurrentMatch ? (
        <span key="last" className="mention-char-match">
          {currentChunk}
        </span>
      ) : (
        <span key="last">{currentChunk}</span>
      ),
    )
  }

  return <>{elements}</>
}

export const MentionPopover = forwardRef<MentionPopoverHandle, MentionPopoverProps>(
  (
    {
      isOpen,
      onClose,
      onSelect,
      position,
      query,
      selectedIndex,
      onSelectedIndexChange,
      items,
      loading = false,
    },
    ref,
  ) => {
    const popoverRef = useRef<HTMLDivElement>(null)
    const listRef = useRef<HTMLDivElement>(null)

    const filteredItems = useMemo(() => {
      return filterAndRankMentionItems(items, query)
    }, [items, query])

    // Expose methods to parent component
    useImperativeHandle(
      ref,
      () => ({
        getDisplayItems: () => filteredItems,
        selectItem: (index: number) => {
          if (filteredItems[index]) {
            onSelect(filteredItems[index])
            onClose()
          }
        },
      }),
      [filteredItems, onSelect, onClose],
    )

    // Keep selected index within valid range when results change
    useEffect(() => {
      if (filteredItems.length === 0) {
        if (selectedIndex !== 0) onSelectedIndexChange(0)
      } else if (selectedIndex >= filteredItems.length) {
        onSelectedIndexChange(Math.max(0, filteredItems.length - 1))
      }
    }, [filteredItems.length, selectedIndex, onSelectedIndexChange])

    // Scroll active item into view
    useEffect(() => {
      if (listRef.current && selectedIndex >= 0 && selectedIndex < filteredItems.length) {
        const selectedElement = listRef.current.children[selectedIndex] as HTMLElement
        if (selectedElement) {
          selectedElement.scrollIntoView({
            block: 'nearest',
            behavior: 'smooth',
          })
        }
      }
    }, [selectedIndex, filteredItems.length])

    // Close on click outside
    useEffect(() => {
      if (!isOpen) return

      const handleClickOutside = (e: MouseEvent) => {
        if (popoverRef.current && !popoverRef.current.contains(e.target as Node)) {
          onClose()
        }
      }

      document.addEventListener('mousedown', handleClickOutside)
      return () => {
        document.removeEventListener('mousedown', handleClickOutside)
      }
    }, [isOpen, onClose])

    if (!isOpen) return null

    const typeBadgeText = (type: MentionItemType) => {
      switch (type) {
        case 'Attachment':
          return 'attached'
        case 'Agent':
          return 'agent'
        case 'Rule':
          return 'rule'
        case 'Directory':
          return 'folder'
        case 'File':
          return 'file'
      }
    }

    return (
      <div
        ref={popoverRef}
        className="mention-popover"
        style={{
          left: `${position.x}px`,
          bottom: `${position.y}px`,
        }}
      >
        <div className="mention-popover-header">
          <span className="mention-header-title">Mention (@)</span>
          {loading ? (
            <span className="mention-loading-indicator">
              <Loader2 size={12} className="spinning" /> Scanning repository…
            </span>
          ) : (
            <span className="mention-header-count">
              {filteredItems.length} {filteredItems.length === 1 ? 'match' : 'matches'}
            </span>
          )}
        </div>

        <div ref={listRef} className="mention-popover-list">
          {filteredItems.length === 0 && !loading && (
            <div className="mention-empty-state">
              <span>No items found matching "@{query}"</span>
            </div>
          )}

          {filteredItems.map((item, index) => {
            const isSelected = index === selectedIndex
            return (
              <div
                key={`${item.itemType}-${item.relativePath}-${item.name}`}
                className={`mention-item ${isSelected ? 'selected' : ''}`}
                onClick={() => {
                  onSelectedIndexChange(index)
                  onSelect(item)
                  onClose()
                }}
                onMouseEnter={() => onSelectedIndexChange(index)}
              >
                <div className="mention-item-icon-box">{renderItemIcon(item)}</div>

                <div className="mention-item-content">
                  <div className="mention-item-top-row">
                    <span className="mention-item-name">
                      {renderHighlightedName(
                        item.name,
                        item.matchedText === item.name ? item.matches : [],
                      )}
                    </span>
                    <span className={`mention-badge mention-badge-${typeBadgeText(item.itemType)}`}>
                      {typeBadgeText(item.itemType)}
                    </span>
                  </div>

                  {item.extra && (
                    <div className="mention-item-extra" title={item.extra}>
                      {renderHighlightedName(
                        item.extra,
                        item.matchedText === item.extra ? item.matches : [],
                      )}
                    </div>
                  )}
                </div>

                {isSelected && (
                  <div className="mention-item-enter-hint">
                    <CornerDownLeft size={11} />
                  </div>
                )}
              </div>
            )
          })}
        </div>

        <div className="mention-popover-footer">
          <span>↑↓ Navigate</span>
          <span>↵ / Tab Select</span>
          <span>Esc Dismiss</span>
        </div>
      </div>
    )
  },
)

MentionPopover.displayName = 'MentionPopover'
