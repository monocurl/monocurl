//
//  EditingBridge.swift
//  Monocurl
//
//  Created by Manu Bhat on 10/9/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation


protocol EditingCModel {
    mutating func up()
}

extension raw_slide_model: EditingCModel {
    mutating func up() {
        if self.dirty == 0 {
            return
        }
        
        self.dirty = 0
        self.scene.pointee.up()
    }
}

extension raw_media_model: EditingCModel { 
    mutating func up() {
        if self.dirty == 0 {
            return
        }
        
        self.dirty = 0
        self.scene.pointee.up()
    }
}

extension raw_scene_model: EditingCModel {
    mutating func up() {
        self.dirty = 0
    }
}

//we have local cache, and child cache, but problem is that if children get disallocated??
//but then we need to see the old tree, hmmm ig we have full tree created from top, and then whenever it needs to be changed, only then do you recache
protocol EditingSwiftCache: Identifiable {
    associatedtype RawCModel: EditingCModel
    
    var ref: UnsafeMutablePointer<RawCModel> {get set}
        
    init(_ p: UnsafeMutablePointer<RawCModel>);
    
    func update(_ p: UnsafeMutablePointer<RawCModel>, force: Bool);
}

extension EditingSwiftCache {
    var id: Int {
        Int(bitPattern: self.ref)
    }
}

struct FunctorArg: Equatable {
    static func == (lhs: FunctorArg, rhs: FunctorArg) -> Bool {
        lhs.location == rhs.location && lhs.line == rhs.line && lhs.tabs == rhs.tabs && lhs.title == rhs.title && lhs.modeIndex == rhs.modeIndex
    }
    
    let location: Int
    let line: Int
    let tabs: Int
    let title: String
    let modeIndex: Int
    let modes: [(title: String, args: [String])]
}

final class EditingSlideCache: ObservableObject, EditingSwiftCache {
    var ref: UnsafeMutablePointer<raw_slide_model>
    
    var index: Int = -1
    var isLast = false
    @Published var title: String!
    @Published var content: String!
    
    @Published var error: String? = nil
    @Published var errorLine: Int = 0
    @Published var ranges: [NSRange] = []
    @Published var functor_pairs: [(start: Int, lines: Int)] = []
    @Published var functor_starts: [FunctorArg] = []
    
    var isDeletable: Bool {
        if index == 0 {
            return false
        }
        else if index == 1 && ref.pointee.scene.pointee.slide_count == 2 {
            return false
        }
        else {
            return true
        }
    }
    
    var isFirst: Bool {
        return index == 0
    }
    
    init(_ ref: UnsafeMutablePointer<raw_slide_model>) {
        self.ref = ref
        self.update(ref, force: true)
    }
    
    func update(_ p: UnsafeMutablePointer<raw_slide_model>, force: Bool) {
        guard force || p.pointee.dirty != 0 || p != self.ref else {
            return
        }
        
        self.ref = p
        
        let raw = p.pointee;
        
        self.index = slide_index_in_parent(ref)
        self.isLast = index == raw.scene.pointee.slide_count - 1
        self.content = String(cString: raw.buffer)

        self.title = index == 0 ? "Config" : "Slide " + String(index)
//        self.title = String(cString: raw.title);
        
        if let m = raw.error.message {
            self.error = String(cString: m)
            self.errorLine = raw.error.line
        }
        else {
            self.error = nil
        }
        
        ranges = []
        for i in 0 ..< raw.total_functor_args {
            ranges.append(NSRange(location: raw.functor_arg_start[i], length: raw.functor_arg_end[i] - raw.functor_arg_start[i]))
        }
        
        functor_pairs = []
        functor_starts = []
        for i in 0 ..< raw.group_count {
            let c = raw.functor_groups[i]
            functor_pairs.append((c.overall_start_index, c.modes[c.current_mode].arg_count))
            
            if c.mode_count > 1 {
                let trueTitle = String(cString: c.title)
                var modes: [(title: String, args: [String])] = []
                for j in 0 ..< c.mode_count {
                    let title = String(cString: c.modes[j].title)
                    let args = (0 ..< c.modes[j].arg_count).map { String(cString: c.modes[j].arg_titles[$0]!) }
                    modes.append((title, args))
                }
                functor_starts.append(FunctorArg(location: c.overall_start_index, line: c.line, tabs: c.tabs, title: trueTitle, modeIndex: c.current_mode, modes: modes))
            }
        }
        
        p.pointee.up()
    }
    
    func writeContent(_ content: String) {
        if content != self.content {
            self.content = content
            slide_write_data(ref, content.utf8CStringPointer, content.count)
        }
    }
}

final class EditingMediaCache: ObservableObject, EditingSwiftCache  {
    var ref: UnsafeMutablePointer<raw_media_model>
    
    @Published var path: URL!
    @Published var name: String!
    @Published var type: raw_media_type!
    
    init(_ ref: UnsafeMutablePointer<raw_media_model>) {
        self.ref = ref
        self.update(ref, force: true)
    }
    
    func update(_ p: UnsafeMutablePointer<raw_media_model>, force: Bool) {
        guard force || p.pointee.dirty != 0 || p != self.ref else {
            return
        }
        
        self.ref = p
        let raw = ref.pointee;
        
        self.path = raw.path == nil ? nil : URL(string: "file://" + String(cString: raw.path).addingPercentEncoding(withAllowedCharacters: .urlPathAllowed)!)!
        self.name = String(cString: raw.name);
        self.type = raw.type
        
        p.pointee.up()
    }
}

final class EditingSceneCache: ObservableObject, EditingSwiftCache {
    var ref: UnsafeMutablePointer<raw_scene_model>
    
    @Published var slides: [EditingSlideCache] = []
    @Published var media: [EditingMediaCache] = []
    
    init(_ ref: UnsafeMutablePointer<raw_scene_model>) {
        self.ref = ref
        self.update(ref, force: true)
    }
    
    func update(_ p: UnsafeMutablePointer<raw_scene_model>, force: Bool) {
        guard force || p.pointee.dirty != 0 || p != self.ref else {
            return
        }
        
        self.ref = p
        
        let raw = ref.pointee;
       
        let slide_force = slides.count != raw.slide_count || force
        
        self.slides.removeLast(max(0, slides.count - raw.slide_count))
        while self.slides.count < raw.slide_count {
            slides.append(EditingSlideCache(raw.slides[slides.count]!))
        }
        
        for i in 0 ..< raw.slide_count {
            self.slides[i].update(raw.slides[i]!, force: slide_force)
        }
        
        let media_force = media.count != raw.media_count || force
        
        media.removeLast(max(0, media.count - raw.media_count))
        while media.count < raw.media_count {
            media.append(EditingMediaCache(raw.media[media.count]!))
        }
        
        for i in 0 ..< raw.media_count {
            media[i].update(raw.media[i]!, force: media_force)
        }
        
        p.pointee.up()
    }
}
