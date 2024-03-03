//
//  EditingSlide.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI

let functor_arg_color = NSColor(red: 1, green: 0.5, blue: 0.4, alpha: 1)
fileprivate var tabWidth: CGFloat = -1;

class ErrorViewController: NSViewController {
    
    let label = NSTextField()
    
    override func loadView() {
        view = NSView()
        view.addSubview(label)
     
        label.translatesAutoresizingMaskIntoConstraints = false
        label.alignment = .center
        label.isEditable = false
        label.isSelectable = false
        label.isBordered = false
        label.drawsBackground = false
        label.backgroundColor = .clear
        
        NSLayoutConstraint.activate([
            label.centerXAnchor.constraint(equalTo: view.centerXAnchor),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
        
        preferredContentSize = label.intrinsicContentSize
        preferredContentSize.width += 30
        preferredContentSize.height += 30
    }
    
    func setMessage(_ str: String) {
        label.stringValue = str
        preferredContentSize = label.intrinsicContentSize
        preferredContentSize.width += 30
        preferredContentSize.height += 30
    }
}

class ErrorView: NSView {
    var error: String? = nil {
        didSet {
            if let e = error {
                self.controller.setMessage(e)
            }
            
            if error != nil && oldValue == nil {
                self.addSubview(image)
            } else if error == nil && oldValue != nil {
                self.image.removeFromSuperview()
                self.closePopover()
            }
        }
    }
    var errorType: slide_error_type = SLIDE_ERROR_SYNTAX
    
    private var isHovering = false {
        didSet {
            needsDisplay = true
        }
    }
    
    private var image: NSImageView
    
    private lazy var controller = ErrorViewController()
    private lazy var popover: NSPopover = {
        let popover = NSPopover()
        popover.behavior = .transient
        popover.contentViewController = controller
        return popover
    }()
    
    override init(frame: NSRect) {
        image = NSImageView(frame: frame)
        let img = NSImage(systemSymbolName: "exclamationmark.square.fill", accessibilityDescription: nil)!
        img.size = frame.size
        image.image = img
        image.imageScaling = .scaleProportionallyUpOrDown
        image.contentTintColor = NSColor.red.withAlphaComponent(0.7)
        
        super.init(frame: frame)
    }
    
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    override func mouseEntered(with event: NSEvent) {
        if self.error != nil {
            isHovering = true
            showPopover()
        }
    }
    
    override func updateTrackingAreas() {
        super.updateTrackingAreas()

        for trackingArea in self.trackingAreas {
            self.removeTrackingArea(trackingArea)
        }
        
        let options: NSTrackingArea.Options = [.mouseEnteredAndExited, .activeAlways]
        let trackingArea = NSTrackingArea(rect: self.bounds, options: options, owner: self, userInfo: nil)
        self.addTrackingArea(trackingArea)
    }
    
    override func mouseExited(with event: NSEvent) {
        isHovering = false
        closePopover()
    }
    
    private func showPopover() {
        popover.show(relativeTo: bounds, of: self, preferredEdge: .maxX)
    }
    
    private func closePopover() {
        popover.close()
    }
}

class FunctorButton: NSView {
    private var arg: FunctorArg!
    override var isFlipped: Bool {
        true
    }
    
    init() {
        super.init(frame: .init())
        
        let names = ["chevron.left", "chevron.right"]
        for (i, name) in names.enumerated() {
            let image = NSImageView(frame: NSRect(origin: .init(x: 1 + 8 * i, y: 1), size: CGSize(width: 10, height: 10)))
            let img = NSImage(systemSymbolName: name, accessibilityDescription: nil)!
            img.size = image.frame.size
            image.image = img
            image.imageScaling = .scaleProportionallyUpOrDown
            image.contentTintColor = NSColor.gray
            
            self.addSubview(image)
        }
    }
    
    override func draw(_ dirtyRect: NSRect) {
        NSColor.gray.withAlphaComponent(0.3).setStroke()
        
        let path = NSBezierPath()
        path.move(to: NSPoint(x: 21, y: 0))
        path.line(to: NSPoint(x: 21, y: bounds.size.height))
        
        path.stroke()
    }
    
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
    
    // very slow, should fix at some point
    func update(from: FunctorArg, in view: MCTextView) {
        if arg == from {
            self.arg = from
            return
        }
        self.arg = from
        
        self.autoresizingMask = .none
        self.translatesAutoresizingMaskIntoConstraints = false
        
        let lm = view.layoutManager!
        let gInd = lm.glyphIndexForCharacter(at: from.location)
        let pos = lm.lineFragmentRect(forGlyphAt: gInd, effectiveRange: nil)
         
        let count = max(1, from.modes[from.modeIndex].args.count)
        self.frame = NSRect(x: pos.minX + tabWidth * CGFloat(from.tabs) - 21, y: pos.minY + 2, width: 23, height: CGFloat(15 * count) - 3)
        needsDisplay = true
    }
    
    func setMode(_ index: Int) {
        guard let tv = self.superview as? MCTextView else {
            return
        }
        
        var it = tv.string.index(tv.string.startIndex, offsetBy: arg.location)
        var count = 0
        var dist = 0
        var currentLines = arg.modes[arg.modeIndex].args.count
        /* enum entry */
        if currentLines == 0 {
            currentLines = 1
        }
        var vals: [String] = []
        var valBuilder = ""
        var hasEncountered = false
        while it < tv.string.endIndex {
            if tv.string[it] == ":" {
                hasEncountered = true
            }
            else if tv.string[it] == "\n" {
                vals.append(valBuilder)
                valBuilder = ""
                hasEncountered = false
                count += 1
                if count == currentLines {
                    dist += 1
                    break
                }
            }
            
            if hasEncountered {
                valBuilder.append(tv.string[it])
            }
            
            dist += 1
            it = tv.string.index(after: it)
        }
        
        var replace = ""
        for (i, title) in arg.modes[index].args.enumerated() {
            replace += String(repeating: "\t", count: arg.tabs)
            replace += title
            if i >= vals.count {
                replace += ": "
            } else {
                replace += vals[i]
            }
            
            replace += "\n"
        }
        
        if arg.modes[index].args.isEmpty {
            replace += String(repeating: "\t", count: arg.tabs)
            replace += arg.title
            replace += ": "
            replace += arg.modes[index].title
            replace += "\n"
        }
        
        tv.forceChange = true
        tv.insertText(replace, replacementRange: NSRange(location: arg.location, length: dist))
        tv.setSelectedRange(NSRange(location: arg.location + replace.count - 1, length: 0))
    }
    
    override func mouseDown(with event: NSEvent) {
        let loc = convert(event.locationInWindow, from: nil)
        if loc.x > self.bounds.width / 2 {
            // increase mode
            self.setMode((self.arg.modeIndex + 1) % self.arg.modes.count)
        }
        else {
            //
            self.setMode((self.arg.modeIndex - 1 + self.arg.modes.count) % self.arg.modes.count)
        }
    }
}

#warning("TODO, very messy and inefficient")
class MCTextView: NSTextView {
    
    var invalidHeight = true
    var heightCache: CGFloat = 0
    var intrinsicHeight: CGFloat {
        if invalidHeight {
            let layoutManager = self.layoutManager!
            layoutManager.ensureLayout(for: textContainer!)
            let usedRect = layoutManager.usedRect(for: textContainer!)
            heightCache = usedRect.size.height
        }
        return heightCache
    }
    
    var model: EditingSlideCache!
    var error: ErrorView!
    var groupLines = 0
    var buttons: [FunctorButton] = []
    
    // yikes, but basically because of writing to the model midway
    // functor ranges get mixed up
    // we definitely have to clean up the FSA at some point
    var forceChange: Bool = false
    
    func initialize() {
        self.allowsUndo = true
        
        self.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        
        let tabWidth = ("    " as NSString).size(withAttributes: [.font: self.font!]).width
        Monocurl.tabWidth = tabWidth
        let defStyle = NSMutableParagraphStyle()
        defStyle.defaultTabInterval = tabWidth
        defStyle.tabStops = []
        self.defaultParagraphStyle = defStyle
        self.typingAttributes[.paragraphStyle] = defStyle
        
        self.backgroundColor = .clear
        self.drawsBackground = false;
       
        self.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        self.textContainer?.heightTracksTextView = false
        self.textContainer?.widthTracksTextView = true
        self.translatesAutoresizingMaskIntoConstraints = false
        
        self.isRichText = false;
        self.isAutomaticTextReplacementEnabled = false;
        self.isAutomaticDataDetectionEnabled = false;
        self.isAutomaticLinkDetectionEnabled = false;
        self.isAutomaticTextCompletionEnabled = false;
        self.isAutomaticDashSubstitutionEnabled = false;
        self.isAutomaticQuoteSubstitutionEnabled = false;
        self.isAutomaticSpellingCorrectionEnabled = false;
        self.turnOffLigatures(nil)
        self.turnOffKerning(nil)
        
        error = ErrorView(frame: NSRect(x: 0, y: 0, width: 20, height: 20))
        self.addSubview(error)
    }
    
    private func select(line at: Int) -> NSRange {
        var totalRange = (self.string as NSString).lineRange(for: NSRange(location: at, length: 0))
        
        // bisect maybe?
        for range in model.ranges {
            if range.lowerBound >= totalRange.upperBound {
                break
            }
            else if range.lowerBound == totalRange.lowerBound {
                totalRange.length -= range.length
                totalRange.location = range.upperBound
                break
            }
        }
        
        var it = self.string.index(self.string.startIndex, offsetBy: totalRange.location)
        while it != self.string.endIndex && self.string[it] == "\t" {
            totalRange.location += 1
            totalRange.length -= 1
            it = self.string.index(after: it)
        }
       
        if totalRange.upperBound < self.string.count || (totalRange.lowerBound < totalRange.upperBound && self.string[self.string.index(self.string.startIndex, offsetBy: totalRange.upperBound - 1)] == "\n") {
            totalRange.length -= 1
        }
        
        return totalRange
    }
    
    override func insertTab(_ sender: Any?) {
        let currentRange = (self.string as NSString).lineRange(for: self.selectedRange())
        let currentSelection = self.select(line: currentRange.location)
        if currentRange.upperBound == self.string.count && !model.isLast {
            self.window?.selectNextKeyView(nil)
        }
        else if currentSelection.lowerBound > self.selectedRange().location {
            self.setSelectedRange(currentSelection)
        }
        else {
            self.setSelectedRange(self.select(line:  currentRange.upperBound))
        }
    }
    
    override func insertBacktab(_ sender: Any?) {
        let currentRange = (self.string as NSString).lineRange(for: self.selectedRange())
        if currentRange.location == 0 && !model.isFirst {
            self.window?.selectPreviousKeyView(nil)
        }
        self.setSelectedRange(self.select(line: max(currentRange.location - 1, 0)))
    }
    
    override func insertNewlineIgnoringFieldEditor(_ sender: Any?) {
        let ns = string as NSString
        let line = ns.lineRange(for: self.selectedRange())
        let lineText = ns.substring(with: line)
        
        let tabs = lineText.prefix { $0 == "\t" }.count
        
        let matchingTabs = line.location != 0 ? "\n"  + String(repeating: "\t", count: tabs) : String(repeating: "\t", count: tabs) + "\n"
        
        let replace = NSRange(location: max(line.location - 1, 0), length: 0)
        if shouldChangeText(in: replace, replacementString: matchingTabs) {
            textStorage?.beginEditing()
            textStorage?.replaceCharacters(in: replace, with: matchingTabs)
            textStorage?.endEditing()
            didChangeText()
            
            self.setSelectedRange(NSRange(location: line.location + matchingTabs.count - 1, length: 0))
        }
    }
    
    override func insertNewline(_ sender: Any?) {
        let ns = string as NSString
        let line = ns.lineRange(for: self.selectedRange())
        let nextLine: NSRange
        if line.lowerBound == self.selectedRange().lowerBound {
            nextLine = line
        } else {
            nextLine = ns.lineRange(for: NSRange(location: line.upperBound, length: 0))
        }
        let lineText = ns.substring(with: line)
        let nextText = ns.substring(with: nextLine)
        
        let tabs = max(lineText.prefix { $0 == "\t" }.count, nextText.prefix { $0 == "\t"}.count)
        let matchingTabs = "\n" + String(repeating: "\t", count: tabs)
        
        if shouldChangeText(in: self.selectedRange(), replacementString: matchingTabs) {
            textStorage?.beginEditing()
            textStorage?.replaceCharacters(in: self.selectedRange(), with: matchingTabs)
            textStorage?.endEditing()
            didChangeText()
        }
    }
    
    // try doing the modification
    // if we get a different result back, abort!
    // not super great, but theres a lot of cases unfortunately
    #warning("TODO, make efficient")
    private func applyTabs(increase: Bool) {
        let range = self.selectedRange()
        let total = self.string as NSString
        var build = self.string
        var line = total.lineRange(for: NSRange(location: max(range.lowerBound, range.upperBound - 1), length: 0))
        let allLines = total.lineRange(for: range)
        var delta = 0
        while true {
            let index = build.index(build.startIndex, offsetBy: line.location)
            if increase {
                build.insert("\t", at: index)
                delta += 1
            }
            else if line.length > 0 && build[index] == "\t" {
                build.remove(at: index)
                delta -= 1
            }
            
            if line.lowerBound <= range.lowerBound {
                break
            }
            else {
                line = total.lineRange(for: NSRange(location: line.lowerBound - 1, length: 0))
            }
        }
        
        model.writeContent(build)
        if model.content == build {
            self.forceChange = true
            syncTextFromState(text: model.content)
            // maybe the change shouldn't have gone through!
            if self.string != model.content {
                model.writeContent(self.string)
            }
            
            var newRange = range
            if increase {
                if allLines.location < range.location {
                    newRange.location += 1
                    if newRange.length > 0 {
                        newRange.length += delta - 1
                    }
                }
                else if newRange.length > 0 {
                    newRange.length += delta
                }
                else {
                    newRange.location += 1
                }
            }
            else {
                newRange.length = max(0, newRange.length + delta)
            }
            self.setSelectedRange(newRange)
        }
        else {
            // do not perform
            model.writeContent(self.string)
            self.syncTextFromState(text: model.content)
        }
    }
    
    override func keyDown(with event: NSEvent) {
        if event.modifierFlags.contains(.command) {
            if event.modifierFlags.contains(.shift)  && event.charactersIgnoringModifiers == "\t" || event.charactersIgnoringModifiers == "[" {
                self.applyTabs(increase: false)
                return
            }
            else if event.charactersIgnoringModifiers == "\t" || event.charactersIgnoringModifiers == "]" {
                self.applyTabs(increase: true)
                return
            }
        }
        
        super.keyDown(with: event)
    }
    
    override func mouseDown(with event: NSEvent) {
        let loc = convert(event.locationInWindow, from: nil)
        for button in buttons {
            if button.bounds.contains(convert(loc, to: button)) {
                button.mouseDown(with: event)
                return
            }
        }
        
        super.mouseDown(with: event)
    }
    
    override func mouseUp(with event: NSEvent) {
        
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        return self
    }
    
    func syncAttributes() {
        let total = NSRange(location: 0, length: string.count)
        self.textStorage?.addAttribute(.foregroundColor, value: NSColor.white, range: total)
        for range in model.ranges {
            guard let range = range.intersection(total) else {
                continue
            }
            
            self.textStorage?.addAttribute(.foregroundColor, value: functor_arg_color, range: range)
        }
    }
    
    func syncGroups() {
        while (buttons.count > model.functor_starts.count) {
            buttons.removeLast().removeFromSuperview()
        }
        while (buttons.count < model.functor_starts.count) {
            buttons.append(FunctorButton())
            self.addSubview(buttons.last!)
        }
        for (start, button) in zip(model.functor_starts, buttons) {
            button.update(from: start, in: self)
        }
    }
    
    func syncTextFromState(text: String) {
        var i = 0
        for (c1, c2) in zip(text, string) {
            if c1 != c2 {
                break
            }
            i += 1
        }
        
        if i == text.count && i == string.count {
            return
        }
        
        var j = 0
        for (c1, c2) in zip(text.reversed(), string.reversed()) {
            if c1 != c2 || text.count == i + j || string.count == i + j {
                break
            }
            j += 1
        }
        
        let total = NSRange(location: i, length: string.count - j - i)
        
        let startIndex = text.index(text.startIndex, offsetBy: i)
        let endIndex = text.index(text.endIndex, offsetBy: -j)

        let unmatchingMiddle = String(text[startIndex..<endIndex])
        
        self.forceChange = true
        if self.shouldChangeText(in: total, replacementString: unmatchingMiddle) {
            self.textStorage?.beginEditing()
            self.string = text
            self.textStorage?.endEditing()
            self.didChangeText()
        }
        
        self.syncAttributes()
    }
    
    func setError(message: String?, line: Int) {
        self.error.error = message
        if message != nil {
            var it = self.string.startIndex
            var count = 0
            var ind = 0
            while it < self.string.endIndex {
                if count == line {
                    let gInd = layoutManager!.glyphIndexForCharacter(at: ind)
                    let rect = layoutManager!.lineFragmentRect(forGlyphAt: gInd, effectiveRange: nil)
                    let midY = rect.midY
                    self.error.frame.origin.y = midY - self.error.frame.height / 2
                    break
                }
                if self.string[it] == "\n" {
                    count += 1
                }
                it = self.string.index(after: it)
                ind += 1
            }
        }
    }
    
    override func layout() {
        super.layout()
        self.error.frame.origin.x = self.bounds.width - 40
    }
}

fileprivate class TextDelegate: NSObject, NSTextStorageDelegate, NSTextViewDelegate {
    
    var um = UndoManager()
    var model: EditingSlideCache
    
    init(model: EditingSlideCache) {
        self.model = model
    }
    
    func textView(_ textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        if commandSelector == #selector(NSResponder.cancelOperation(_:)) {
            textView.window?.makeFirstResponder(nil)
            return true;
        }
        else if commandSelector == #selector(NSResponder.moveUp(_:)) {
            var perform = textView.string.isEmpty ||  textView.selectedRange().location == 0
            if !perform {
                let loc = textView.selectedRange().location
                if loc != textView.string.count {
                    let lm = textView.layoutManager!
                    
                    let lr = lm.lineFragmentRect(forGlyphAt: loc, effectiveRange: nil)
                    let comp = lm.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
                    perform = lr == comp
                }
            }
            
            if perform {
                if !model.isFirst {
                    textView.window?.selectPreviousKeyView(nil);
                    return true;
                }
            }
        }
        else if commandSelector == #selector(NSResponder.moveDown(_:)) {
            var perform = textView.string.isEmpty || textView.selectedRange().location == textView.string.count
            if !perform {
                let loc = textView.selectedRange().location
                let lm = textView.layoutManager!
                
                let lr = lm.lineFragmentRect(forGlyphAt: loc, effectiveRange: nil)
                perform = lr.maxY == textView.bounds.maxY
            }
            
            if perform {
                if !model.isLast {
                    textView.window?.selectNextKeyView(nil);
                    return true;
                }
            }
        }

        return false;
    }
    
    func removePartialIntersections(range: NSRange) -> NSRange? {
        var range = range
        for r in model.ranges {
            if r.lowerBound > range.upperBound {
                break
            }
            
            if r.contains(range.lowerBound) {
                if r.upperBound >= range.upperBound {
                    return nil
                }
                else {
                    range.length -= (r.upperBound - range.lowerBound)
                    range.location = r.upperBound
                }
            }
            else if r.contains(range.upperBound) {
                /* note that exclusion regions cannot start at 0 by definition */
                range.location -= range.upperBound - (r.lowerBound - 1)
                range.length -= range.upperBound - (r.lowerBound - 1)
            }
        }
        
        if range.length < 0 || range.location < 0 {
            return nil
        }
        
        return range
    }
    
    func textView(_ textView: NSTextView, shouldChangeTextIn affectedCharRange: NSRange, replacementString: String?) -> Bool {
        let textView = textView as! MCTextView
        if um.isUndoing || um.isRedoing || textView.forceChange {
            textView.forceChange = false
            let newText = (textView.string as NSString).replacingCharacters(in: affectedCharRange, with: replacementString ?? "")
            
            model.writeContent(newText)
            // make sure that we are not written the corrected version
            if model.content != newText {
                model.content = newText
            }
            return true
        }
        
        /* check if intersects partially with reserved */
        let newRange = self.removePartialIntersections(range: affectedCharRange)
        if let newRange = newRange, newRange != affectedCharRange {
            if textView.shouldChangeText(in: newRange, replacementString: replacementString) {
                textView.textStorage?.beginEditing()
                textView.textStorage?.replaceCharacters(in: newRange, with: replacementString ?? "")
                textView.textStorage?.endEditing()
                textView.didChangeText()
            }
            
            return false
        }
        else if newRange == nil {
            return false
        }
        
        let newText = (textView.string as NSString).replacingCharacters(in: affectedCharRange, with: replacementString ?? "")
        
        model.writeContent(newText)
        let res = model.content!
        if res != newText {
            let old = affectedCharRange
            textView.syncTextFromState(text: res)
            textView.setSelectedRange(NSRange(location: old.location + (replacementString?.count ?? 0), length: 0))
            return false
        }
        else {
            return true
        }
    }
    
    func undoManager(for view: NSTextView) -> UndoManager? {
        um
    }
    
    func textDidChange(_ notification: Notification) {
        guard let text = notification.object as? MCTextView else {
            return
        }
        
        text.syncAttributes()
        text.syncGroups()
        text.invalidHeight = true
        text.superview?.invalidateIntrinsicContentSize()
    }
}

class ViewWrapper: NSView {
    var textView: MCTextView?
    var lastWidth: CGFloat = -1

    override func layout() {
        if bounds.width != lastWidth {
            self.invalidateIntrinsicContentSize()
            textView?.invalidHeight = true
            lastWidth = bounds.width
        }
        super.layout()
        textView?.frame = bounds
    }

    override var intrinsicContentSize: NSSize {
        return NSSize(width: 0, height: textView?.intrinsicHeight ?? 0)
    }
}

fileprivate struct MCTextField: NSViewRepresentable {
    @ObservedObject var data: EditingSlideCache
    
    func makeCoordinator() -> TextDelegate {
        TextDelegate(model: data)
    }
    
    func makeNSView(context: Context) -> ViewWrapper {
        let wrapper = ViewWrapper()
        let field = MCTextView()
        field.delegate = context.coordinator
        field.initialize()
        field.model = data
        field.string = data.content
        field.syncAttributes()
        wrapper.textView = field
        wrapper.addSubview(field)
        return wrapper
    }
    
    func updateNSView(_ wrapper: ViewWrapper, context: Context) {
        guard let text = wrapper.textView else {
            return
        }
        
        context.coordinator.model = data
        text.model = data
        if text.string != data.content {
            text.syncTextFromState(text: data.content)
        }
        text.syncAttributes()
        text.syncGroups()
        text.setError(message: data.error, line: data.errorLine)
    }
}

struct EditingSlide: View {
    @ObservedObject var state: EditingSlideCache
    
    @Binding var showingConfirmation: Bool
    @Binding var deletionConfirmation: UnsafeMutablePointer<raw_slide_model>?
    
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 4) {
                Text(state.title)
                    .font(.title3)
                
                if (state.isDeletable) {
                    Button {
                        deletionConfirmation = state.ref
                        showingConfirmation = true
                    } label: {
                        Image(systemName: "trash")
                    }
                    .buttonStyle(.link)
                    .padding(.bottom, 2)
                }
            }
            
            ZStack {
                VStack(spacing: 0) {
                    Divider()
                    Rectangle()
                        .fill(Color(red: 0.2, green: 0.2, blue: 0.2))
                }
                
                HStack {
                    Rectangle()
                        .fill(.yellow)
                        .frame(maxWidth: 2)
                    ZStack(alignment: .bottom) {
                        MCTextField(data: state)
                            .padding(.top, 6)
                            .padding(.bottom, 10)

                        Button {
                            insert_slide_after(self.state.ref)
                        } label: {
                            Image(systemName: "plus.square")
                        }
                        .buttonStyle(.link)
                        .padding(4)
                    }
                }
            }
            .padding(.horizontal, 20)
        }
        .padding(.top, 10)
    }
}
