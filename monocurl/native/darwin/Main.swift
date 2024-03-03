//
//  GMMMain.swift
//  Monocurl
//
//  Created by Manu Bhat on 9/6/22.
//  Copyright © 2022 Enigmadux. All rights reserved.
//
import Cocoa
import Carbon
import Foundation
import SwiftUI
import Metal
import UniformTypeIdentifiers

extension NSApplication {
    static func removeFirstResponder() {
        NSApplication.shared.windows.first?.makeFirstResponder(nil)
    }
}

class AppDelegate: NSObject, NSApplicationDelegate {
    static var gpu: MTLDevice!
    
    var storage: StorageEnvironment!
    var environment: MonocurlEnvironment!
    
    func applicationWillFinishLaunching(_ notification: Notification) {
        monocurl_init();
        
        slide_flush = cSlideCallback
        scene_flush = cSceneCallback
        
        timeline_flush = cTimelineCallback
        viewport_flush = cViewportCallback
        
        free_buffer = cFreeBuffer
        poll_texture = cPollTexture
        
        path_translation = translateBookmark
        std_lib_path = stdLibPath
        default_scene_path = defaultScenePath
        tex_binary_path = texPath
        tex_intermediate_path = texIntermediatePath
        
        export_frame = cWriteFrame
        export_finish = cFinishExport
        
        // takes a VERY long time to load, can we reduce this somehow?
        if let first = MTLCopyAllDevices().first {
            Self.gpu = first
        }
        else {
            Self.gpu = MTLCreateSystemDefaultDevice()
        }
    }
    
    func applicationWillTerminate(_ notification: Notification) {
        if let scene = self.environment.scene {
            scene_handle_free(scene);
            self.environment.scene = nil;
        }
        
        monocurl_free();
    }
}

@main
struct AppRoot: App {
    public static let name: String = "Monocurl"
    
    public static let sceneExtension: String = "mcf"
    public static let sceneType: UTType = UTType(filenameExtension: sceneExtension)!
  
    @NSApplicationDelegateAdaptor(AppDelegate.self) fileprivate var delegate
   
    @StateObject private var storage = StorageEnvironment();
    @StateObject private var environment = MonocurlEnvironment()
    
    var body: some Scene {
        WindowGroup {
            Router()
                .frame(minWidth: 1200, maxWidth: .infinity, minHeight: 800, maxHeight: .infinity)
                .environmentObject(self.storage)
                .environmentObject(self.environment)
                .onAppear {
                    delegate.storage = storage;
                    delegate.environment = environment;
                }
                .onAppear {
                    NSApplication.shared.windows.first?.collectionBehavior = .fullScreenPrimary
                    NSApplication.shared.presentationOptions = .fullScreen
                    
                    NSEvent.addLocalMonitorForEvents(matching: .keyDown) {
                        if Int($0.keyCode) == kVK_Escape && environment.inPresentationMode {
                            environment.togglePresentaiton()
                            return nil
                        }
                        else {
                            return $0
                        }
                    }
                }
        }
        .commands {
            /* technically some commands induce race conditions... */
            CommandGroup(replacing: .newItem) {
                Button("Force Save") {
                    if environment.scene != nil {
                        file_write_model(self.environment.scene);
                    }
                }
                .keyboardShortcut(KeyEquivalent("S"), modifiers: .command)

                Button("Import") {
                    if self.storage.tryImport() != nil {
//                        self.environment.moveToRoute(Route(routeType: .landing))
                    }
                }
                .keyboardShortcut(KeyEquivalent("I"), modifiers: .command)
            }
            
            CommandGroup(replacing: .toolbar) {
                Button("Toggle Play") {
                    if self.environment.scene != nil && can_toggle_play(self.environment.scene.pointee.timeline) == 1 {
                        timeline_play_toggle(self.environment.scene.pointee.timeline)
                    }
                }
                .keyboardShortcut(KeyEquivalent(" "), modifiers: [])
                
                Button("Prev Slide") {
                    if self.environment.scene != nil && can_prev_slide(self.environment.scene.pointee.timeline) == 1 {
                        let timeline = self.environment.scene.pointee.timeline
                        if (timeline!.pointee.timestamp.offset < Double.ulpOfOne && timeline!.pointee.timestamp.slide > 1) {
                            timeline_seek_to(timeline, timestamp(slide: timeline!.pointee.timestamp.slide - 1, offset: 0), 1)
                        }
                        else {
                            timeline_seek_to(timeline, timestamp(slide: timeline!.pointee.timestamp.slide, offset: 0), 1)
                        }
                    }
                }
                .keyboardShortcut(KeyEquivalent(","), modifiers: [])
                
                Button("Next Slide") {
                    if self.environment.scene != nil && can_next_slide(self.environment.scene?.pointee.timeline) == 1 {
                        let timeline = self.environment.scene.pointee.timeline
                        timeline_seek_to(timeline, timestamp(slide: timeline!.pointee.timestamp.slide + 1, offset: 0), 1)
                    }
                }
                .keyboardShortcut(KeyEquivalent("."), modifiers: [])
                
                Button("Full Start") {
                    if self.environment.scene != nil && can_revert_full(self.environment.scene?.pointee.timeline) == 1 {
                        timeline_seek_to(self.environment.scene.pointee.timeline, timestamp(slide: 1, offset: 0), 1)
                    }
                }
                .keyboardShortcut(KeyEquivalent("<"), modifiers: [])
                
                Button("Full End") {
                    if self.environment.scene != nil && can_revert_full(self.environment.scene?.pointee.timeline) == 1 {
                        timeline_seek_to(self.environment.scene.pointee.timeline, timestamp(slide: self.environment.scene.pointee.timeline.pointee.executor.pointee.slide_count, offset: 0), 1)
                    }
                }
                .keyboardShortcut(KeyEquivalent(">"), modifiers: [])
                
                Button("Toggle Presentation") {
                    self.environment.togglePresentaiton()
                }
                .keyboardShortcut(KeyEquivalent("f"), modifiers: .control)
            }
        }
    }
}
