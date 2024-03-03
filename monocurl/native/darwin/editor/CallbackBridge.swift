//
//  CallbackBridge.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/27/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//

import Foundation
import SwiftUI

//create stack
var store: EditingSceneCache!
var timelineStore: Binding<TimelineCache>!
var viewportStore: Binding<ViewportCache>!

weak var bufferManager: BufferManager!
weak var textureManager: TextureManager!
weak var exportManager: ExportController!

fileprivate func findScene(ref: UnsafeMutablePointer<raw_scene_model>) -> EditingSceneCache! {
    //re write each change, so that undo state is updated
    return store;
}

fileprivate func findSlide(ref: UnsafeMutablePointer<raw_slide_model>) -> EditingSlideCache! {
    let scene = findScene(ref: ref.pointee.scene)

    if let p = ref.pointee.scene {
        for i in 0 ..< p.pointee.slide_count {
            if (p.pointee.slides[i] == ref) {
                return scene?.slides[i];
            }
        }
    }
 
    return nil
}

func runMain(_ closure: @escaping () -> ()) {
    if Thread.isMainThread {
        closure()
    }
    else {
        DispatchQueue.main.async(execute: closure)
    }
}

fileprivate func _cSlideCallback(_ ref: UnsafeMutablePointer<raw_slide_model>!, is_global: CChar) {

    runMain {
        if store == nil {
            return
        }
        let binding = findSlide(ref: ref)!
        binding.update(ref, force: false)
    }
}
let cSlideCallback: @convention(c) (UnsafeMutablePointer<raw_slide_model>?, CChar) -> Void = _cSlideCallback(_:is_global:);

fileprivate func _cSceneCallback(_ ref: UnsafeMutablePointer<raw_scene_model>!, is_global: CChar) {

    runMain {
        if store == nil {
            return
        }
        let binding = findScene(ref: ref)!
        binding.update(ref, force: false);
    }
}
let cSceneCallback: @convention(c) (UnsafeMutablePointer<raw_scene_model>?, CChar) -> Void = _cSceneCallback(_:is_global:);

fileprivate func _cViewportCallback(_ ref: UnsafeMutablePointer<viewport>!) {
    let cache = ViewportCache(ref, nonce: (viewportStore?.wrappedValue.nonce ?? 0) + 1);
    
    DispatchQueue.main.async {
        if let export = exportManager {
            export.cache = cache;
        }
        
        if viewportStore == nil {
            return
        }
        
        viewportStore.wrappedValue = cache;
    }
}
let cViewportCallback: @convention(c) (UnsafeMutablePointer<viewport>?) -> Void = _cViewportCallback(_:);

fileprivate func _cTimelineCallback(_ ref: UnsafeMutablePointer<timeline>!) {
    let cache = TimelineCache(ref);
    
    DispatchQueue.main.async {
        /* during closing, the async seems to matter */
        if timelineStore == nil {
            return
        }
        
        timelineStore.wrappedValue = cache;
    }
}
let cTimelineCallback: @convention(c) (UnsafeMutablePointer<timeline>?) -> Void = _cTimelineCallback(_:);

fileprivate func _cFreeBuffer(handle: UInt32) {
    bufferManager?.free(id: handle)
}
let cFreeBuffer: @convention(c) (UInt32) -> Void = _cFreeBuffer(handle:)

fileprivate func _cPollTexture(path: UnsafePointer<CChar>!) -> UInt32 {
    let string = String(cString: path)
    let url = URL(fileURLWithPath: string)
    
    return textureManager!.pollID(url: url);
}
let cPollTexture: @convention(c) (UnsafePointer<CChar>?) -> UInt32 = _cPollTexture(path:)

fileprivate func _cWriteFrame(timeline: UnsafePointer<timeline>!) {
    DispatchQueue.main.async {
        exportManager.draw();
    }
}
let cWriteFrame: @convention(c) (UnsafePointer<timeline>?) -> Void = _cWriteFrame(timeline:)

fileprivate func _cFinishExport(timeline: UnsafePointer<timeline>!, _ error: UnsafePointer<CChar>!) {
    //this is an annoying ui bug
    //where it won't show error if it's displayed as sheet is showing
    //so wait until sheet is fully displayed
    DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) {
        exportManager.exportFinish(error: error)
    }
}
let cFinishExport: @convention(c) (UnsafePointer<timeline>?, UnsafePointer<CChar>?) -> Void = _cFinishExport(timeline:_:)

fileprivate func _translateBookmark(path: UnsafePointer<CChar>!) -> UnsafePointer<CChar>! {
    let base64 = String(cString: path);
    
    guard let datautf = base64.data(using: .utf8), let dataRaw = Data(base64Encoded: datautf) else {
        NSLog("Could not convert bookmark to data")
        return nil
    }
    
    guard let url = try? NSURL(resolvingBookmarkData: dataRaw, options: .withSecurityScope, relativeTo: nil, bookmarkDataIsStale: nil) else {
        NSLog("Could not convert data to NSURL")
        return nil;
    }
   
    #warning("TODO this is a memory leak... We'll see best way to fix this")
    url.startAccessingSecurityScopedResource()
    return UnsafePointer(Monocurl.path(for: url as URL).utf8CStringPointer)
}
let translateBookmark: @convention(c) (UnsafePointer<CChar>?) -> UnsafePointer<CChar>? = _translateBookmark(path:)


fileprivate let _defaultScene: UnsafePointer<CChar>? = {
    if let url = Bundle.main.url(forResource: "mc_default_scene", withExtension: "mcf") {
        return UnsafePointer(path(for: url).utf8CStringPointer)
    }
    return nil
}()

let defaultScenePath: @convention(c) () -> UnsafePointer<CChar>? = {
    return _defaultScene
}

fileprivate let _stdLibPath: UnsafePointer<CChar>? = {
    if let url = Bundle.main.url(forResource: "libmc", withExtension: "mcf") {
        return UnsafePointer(path(for: url).utf8CStringPointer)
    }
    return nil
}()

let stdLibPath: @convention(c) () -> UnsafePointer<CChar>? = {
    return _stdLibPath
}

fileprivate let _texPath: UnsafePointer<CChar>? = {
    if let url = Bundle.main.url(forResource: "latex", withExtension: nil, subdirectory: "tex-live/bin/universal-darwin") {
        var path = path(for: url.deletingLastPathComponent())
        if (!path.hasSuffix("/")) {
            path.append("/")
        }
        
        return UnsafePointer(path.utf8CStringPointer)
    }
    return nil
}()

let texPath: @convention(c) () -> UnsafePointer<CChar>? = {
    return _texPath
}

fileprivate let _texIntermediatePath: UnsafePointer<CChar>? = {
    if let url = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first {
        let monocurl_dir = url.appendingPathComponent("monocurl/tex/")
        
        if (!FileManager.default.fileExists(atPath: monocurl_dir.path)) {
            do {
                try FileManager.default.createDirectory(at: monocurl_dir, withIntermediateDirectories: true, attributes: nil)
            } catch {
                return nil;
            }
        }
       
        var path = path(for: monocurl_dir)
        if (!path.hasSuffix("/")) {
            path.append("/")
        }
        return UnsafePointer(path.utf8CStringPointer)
    }
    return nil
}()

let texIntermediatePath: @convention(c) () -> UnsafePointer<CChar>? = {
    return _texIntermediatePath
}

