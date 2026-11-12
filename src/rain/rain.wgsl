// Halcyon wallpaper weather. Built after glass.wgsl, whose lens outline and
// lighting it uses. Analytic drops, no ray marching or GPU readback.
//
// Rain is two passes plus composition:
// - a reduced-resolution water field, premultiplied by coverage: shaped lens
//   coordinates (xy), radius (z, negative for water that does not wipe the
//   glass, such as beads and fresh impacts) and coverage (w). Small lens
//   quads for the moving and fresh drops, blended in order. The static beads
//   are cached once in the same encoding;
// - a condensation field at a quarter of the water field's size that keeps
//   its previous frame: moving drops wipe it clear and it fogs over again in
//   4 to 30 seconds. It also carries the mist-blurred wallpaper the fog
//   shows, so composition reads both in one sample;
// - full-resolution composition: the beads the condensation allows under
//   this frame's drops, sharp wallpaper refracted through each drop, fogged
//   glass between them, rim shading and highlights.
struct Params {
    frame: vec4<f32>, // aspect, time, intensity, size
    motion: vec4<f32>, // speed, wind, mist, refraction
    style: vec4<f32>, // mode, opacity, trails, fade
    crop: vec4<f32>, // visible UV origin/extent; scroll clipping keeps the image fixed
    atmosphere: vec4<f32>, // rain: elapsed seconds, field texel, detail; others: opacity, height, softness
    tint: vec4<f32>, // rgb; w: one output pixel in UV height
};
@group(0) @binding(0) var<uniform> p: Params;
@group(1) @binding(0) var background: texture_2d<f32>;
@group(1) @binding(1) var linear_sampler: sampler;
@group(1) @binding(2) var dynamic_field: texture_2d<f32>;
@group(1) @binding(3) var condensation: texture_2d<f32>;
@group(1) @binding(4) var beads: texture_2d<f32>;
struct Vertex { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32> };
fn triangle(index:u32) -> Vertex {
    let positions=array<vec2<f32>,3>(vec2(-1.,-1.),vec2(3.,-1.),vec2(-1.,3.));
    let xy=positions[index];
    return Vertex(vec4(xy,0.,1.), vec2(xy.x*0.5+0.5,0.5-xy.y*0.5));
}
@vertex fn fullscreen(@builtin(vertex_index) index:u32) -> Vertex { return triangle(index); }
@vertex fn composite_vertex(@builtin(vertex_index) index:u32) -> Vertex {
    var v=triangle(index);
    v.uv=p.crop.xy+v.uv*p.crop.zw;
    return v;
}
// Hash without Sine, David Hoskins (MIT): https://www.shadertoy.com/view/4djSRW
fn hash(n:vec2<f32>) -> f32 {
    let h=fract(vec3(n.x,n.y,n.x)*vec3(0.1031,0.1030,0.0973));
    let q=h+dot(h,h.yzx+33.33);
    return fract((q.x+q.y)*q.z);
}
fn hash3(n:vec2<f32>) -> vec3<f32> {
    var h=fract(vec3(n.x,n.y,n.x)*vec3(0.1031,0.1030,0.0973));
    h+=dot(h,h.yxz+33.33);
    return fract((h.xxy+h.yzz)*h.zyx);
}

// One encoded lens. `wipes` is false for water that stays on the glass.
fn encode(q:vec2<f32>, cover:f32, radius:f32, wipes:bool) -> vec4<f32> {
    return vec4(q*cover, select(-radius, radius, wipes)*cover, cover);
}
fn over(top:vec4<f32>, bottom:vec4<f32>) -> vec4<f32> {
    return top+bottom*(1.-clamp(top.w,0.,1.));
}
fn lens_cover(q:vec2<f32>, radius:f32) -> f32 {
    let edge=min(0.7, 1.5*p.atmosphere.y/max(radius,1e-5));
    return 1.-smoothstep(1.-edge,1.+edge,dot(q,q));
}
fn drop_scale() -> f32 { return 0.55+p.frame.w*1.1; }
fn pace() -> f32 { return 0.3+p.motion.x*1.4; }

// Condensation beads, plus sparser beads that sliding drops leave behind in
// their cleared paths (negative radius). Drawn once into a cached texture.
@fragment fn static_beads(v:Vertex) -> @location(0) vec4<f32> {
    let xy=v.uv*vec2(p.frame.x,1.);
    let amount=p.frame.z;
    var field=vec4(0.);
    if amount<=0. {return field;}
    let size=0.75+p.frame.w*0.5;
    // Eco skips the two finest, densest layers.
    for(var layer=select(2,0,p.atmosphere.z>0.);layer<4;layer+=1) {
        let residue=layer==3;
        let tile=select(array<f32,4>(0.0125,0.0175,0.024,0.03)[layer],0.03,residue)*size;
        let grid=xy/tile+vec2(f32(layer)*0.37,f32(layer)*0.61);
        let cell=floor(grid);
        let seed=hash3(cell+vec2(17.*f32(layer),5.));
        let share=select(0.25+amount*0.65,0.15+amount*0.35,residue)*select(1.,0.7,layer==2);
        if seed.x>share {continue;}
        let radius=tile*select(mix(0.10,0.30,seed.z*seed.z),mix(0.09,0.20,seed.z),residue);
        let center=vec2(0.5)+(seed.yz*2.-1.)*(0.5-radius/tile*1.3);
        let raw=(fract(grid)-center)*tile/vec2(radius,radius*mix(0.82,1.08,seed.y));
        let q=lens_shape(raw,seed,0.,0.);
        let cover=lens_cover(q,radius)*select(0.92,0.8,residue);
        if cover<=0. {continue;}
        field=over(encode(q,cover,radius,!residue),field);
    }
    return field;
}

// Moving and fresh water is drawn as one small quad per drop, so the cost
// follows the wet area rather than the field size. A quad's corners are in
// lens radii; the fragment shapes the outline from them.
struct LensVertex {
    @builtin(position) position:vec4<f32>,
    @location(0) raw:vec2<f32>,
    @location(1) @interpolate(flat) traits:vec3<f32>,
    @location(2) @interpolate(flat) shape:vec4<f32>, // splash, taper, outline weight, radius
    @location(3) @interpolate(flat) look:vec2<f32>, // opacity, wipes (1) or stays (0)
};
fn hidden() -> LensVertex {
    return LensVertex(vec4(-2.,-2.,0.,1.),vec2(0.),vec3(0.),vec4(0.),vec2(0.));
}
// `center` is in glass units (x scaled by the aspect); `shear` slants the quad
// with the wind so the lens follows its lane.
fn lens_vertex(index:u32, center:vec2<f32>, radius:f32, stretch:f32, shear:f32,
    traits:vec3<f32>, shape:vec4<f32>, look:vec2<f32>) -> LensVertex {
    let corners=array<vec2<f32>,6>(vec2(-1.,-1.),vec2(1.,-1.),vec2(-1.,1.),vec2(-1.,1.),vec2(1.,-1.),vec2(1.,1.));
    // Room for the irregular outline, a teardrop's wider foot and the rim.
    let edge=min(0.7,1.5*p.atmosphere.y/max(radius,1e-5));
    let raw=corners[index]*(vec2(1.6,1.45)+edge);
    let dy=raw.y*radius*stretch;
    let xy=(center+vec2(raw.x*radius+shear*dy,dy))/vec2(p.frame.x,1.);
    return LensVertex(vec4(xy.x*2.-1.,1.-xy.y*2.,0.,1.),raw,traits,shape,look);
}
@fragment fn lens_fragment(v:LensVertex) -> @location(0) vec4<f32> {
    let q=mix(v.raw,lens_shape(v.raw,v.traits,v.shape.x,v.shape.y),v.shape.z);
    let cover=lens_cover(q,v.shape.w)*v.look.x;
    if cover<=0. {discard;}
    return encode(q,cover,v.shape.w,v.look.y>0.5);
}

// Runoff: in each lane a drop lands, grows, holds, then slides off the glass
// in fits and starts along a winding path. Wiping drops clear the fog.
fn runoff(index:u32, slot_index:u32, width:f32, scale:f32, seed:f32, slot:f32) -> LensVertex {
    // Lanes run along x - shear*y, slanted by the wind.
    let shear=p.motion.y*0.3;
    let lane=floor(min(0.,-shear)/width)-1.+f32(slot_index);
    if lane*width>p.frame.x+max(0.,-shear)+width {return hidden();}
    let stream=hash3(vec2(lane,seed));
    let speed=mix(0.16,0.42,stream.x)*pace();
    let hold=mix(0.8,5.5,stream.z)/pace();
    let period=hold+1.3/speed;
    let phase=p.frame.y/period+stream.y+slot;
    let cycle=floor(phase);
    let traits=hash3(vec2(lane*1.7+seed*53.,cycle+slot*31.));
    if traits.x>0.12+p.frame.z*0.88 {return hidden();}
    let elapsed=fract(phase)*period;
    let growth=smoothstep(0.,0.5,elapsed);
    let radius=0.016*scale*drop_scale()*mix(0.65,1.,traits.y)*growth;
    if radius<1e-4 {return hidden();}
    let slip=max(0.,elapsed-hold);
    // Stick-slip: the velocity dips close to zero between surges.
    let surge=mix(2.5,6.5,traits.z);
    let descent=speed*(slip+0.82*sin(surge*slip)/surge);
    let center_y=0.04+traits.z*0.7+descent;
    if center_y-radius*2.>1. {return hidden();}
    let sliding=smoothstep(0.,0.3,slip);
    let velocity=sliding*(1.+0.82*cos(surge*slip));
    let sway=width*0.2;
    let drift=sway*(sin(center_y*mix(5.,11.,stream.y)+stream.z*6.283)
        +0.35*sin(center_y*23.+traits.y*9.));
    let room=max(0.,width*0.5-radius*1.6-sway*1.35);
    let x=(lane+0.5)*width+(traits.y*2.-1.)*room*0.7+drift+shear*center_y;
    // Runoff is rounder than a fresh splash: soften the outline's lobes.
    return lens_vertex(index,vec2(x,center_y),radius,1.+0.2*velocity,shear,traits,
        vec4(1.-growth,0.32*sliding,0.35,radius),vec2(1.,select(0.,1.,slip>0.)));
}

// Impacts: fresh drops land, sit, and dry up; they do not wipe.
fn impact(index:u32, cell_index:u32, tile:f32, seed:f32) -> LensVertex {
    let columns=u32(ceil(p.frame.x/tile))+3u;
    if cell_index>=columns*(u32(ceil(1./tile))+3u) {return hidden();}
    let cell=floor(vec2(seed))-1.+vec2(f32(cell_index%columns),f32(cell_index/columns));
    let stream=hash3(cell+vec2(seed*7.,41.));
    let life=mix(5.,12.,stream.x)/pace();
    let phase=p.frame.y/life+stream.y;
    let traits=hash3(cell*1.31+vec2(floor(phase)*7.1,seed));
    if traits.x>p.frame.z*0.75 {return hidden();}
    let age=fract(phase);
    let growth=smoothstep(0.,0.03,age);
    let fade=1.-smoothstep(0.7,1.,age);
    let radius=tile*mix(0.07,0.18,traits.y*traits.y)*drop_scale()*growth;
    if radius<1e-4 {return hidden();}
    let center=vec2(0.5)+(traits.yz*2.-1.)*0.25;
    return lens_vertex(index,(cell+center-seed)*tile,radius,1.,0.,traits,
        vec4(1.-smoothstep(0.,0.15,age),0.,1.,radius),vec2(fade,0.));
}

// Procedural rain's instances, in drawing order: two impact grids, then three
// sets of runoff lanes. gpu.rs (`procedural_draws`) draws only the slots that
// can reach the glass; any that still fall outside are hidden. Eco skips the
// finer impacts and runoff.
const CELLS:u32=8192u;
const LANES:u32=512u;
@vertex fn procedural_vertex(@builtin(vertex_index) index:u32, @builtin(instance_index) instance:u32) -> LensVertex {
    var i=instance;
    if i<CELLS {return impact(index,i,0.05,0.);}
    i-=CELLS;
    let detail=p.atmosphere.z>0.;
    if i<CELLS {
        if !detail {return hidden();}
        return impact(index,i,0.034,3.7);
    }
    i-=CELLS;
    if i<LANES {
        if !detail {return hidden();}
        return runoff(index,i,0.034,0.55,19.,0.);
    }
    i-=LANES;
    if i<LANES {return runoff(index,i,0.07,1.,7.,0.);}
    return runoff(index,i-LANES,0.07,1.,7.,0.5);
}

// Simulated drops from the CPU: x, y, radius, stretch; opacity, taper
// (negative for beads that stay) and a seed for the outline.
@vertex fn drop_vertex(@builtin(vertex_index) index:u32,@location(0) drop:vec4<f32>,@location(1) look:vec3<f32>) -> LensVertex {
    let traits=hash3(vec2(look.z*91.7,look.z*13.3));
    return lens_vertex(index,drop.xy,drop.z,drop.w,0.,traits,
        vec4(0.,max(look.y,0.),1.,drop.z),vec2(look.x,select(0.,1.,look.y>=0.)));
}

// Condensation feedback: rgb is the blurred wallpaper, a the fog (1 fogged,
// 0 wiped clear). `water` is the water field.
@group(1) @binding(0) var previous: texture_2d<f32>;
@group(1) @binding(1) var feedback_sampler: sampler;
@group(1) @binding(2) var water: texture_2d<f32>;
@group(1) @binding(3) var wallpaper: texture_2d<f32>;

@fragment fn condense(v:Vertex) -> @location(0) vec4<f32> {
    // Fogged glass shows the wallpaper blurred by the mist.
    let blur=textureSampleLevel(wallpaper,feedback_sampler,v.uv,2.5+2.*p.motion.z).rgb;
    let last=textureSampleLevel(previous,feedback_sampler,v.uv,0.).a;
    if p.style.z<=0. {return vec4(blur,1.);}
    let step=0.25/vec2<f32>(textureDimensions(previous));
    var wipe=0.;
    for(var i=0;i<4;i+=1) {
        let o=vec2(select(-1.,1.,(i&1)==1),select(-1.,1.,i>=2))*step;
        let f=textureSampleLevel(water,feedback_sampler,v.uv+o,0.);
        if f.z>0. {wipe=max(wipe,clamp(f.w,0.,1.));}
    }
    // Trails decide how long a cleared path stays clear.
    let settle=4.+26.*p.style.z;
    let fogged=min(1.,last+p.atmosphere.x/settle);
    return vec4(blur,min(fogged,1.-smoothstep(0.12,0.55,wipe)));
}

fn decode(f:vec4<f32>) -> Water {
    let weight=max(f.w,1e-4);
    var w=lens_light(f.xy/weight,clamp(f.w,0.,1.));
    w.bend*=abs(f.z)/weight;
    return w;
}
fn luminance(c:vec3<f32>) -> f32 { return dot(c,vec3(0.2126,0.7152,0.0722)); }

// Each effect composes in its own entry point, so rain's registers are not
// sized for the snow loop and the reverse.
fn tinted(uv:vec2<f32>, amount:f32) -> vec4<f32> {
    let clean=textureSampleLevel(background,linear_sampler,uv,0.).rgb;
    return vec4(mix(clean,p.tint.rgb,clamp(amount,0.,1.)*p.style.w),1.);
}
@fragment fn compose_snow(v:Vertex)->@location(0) vec4<f32> {
    return tinted(v.uv,snow(v.uv));
}
@fragment fn compose_atmosphere(v:Vertex)->@location(0) vec4<f32> {
    return tinted(v.uv,textureSampleLevel(dynamic_field,linear_sampler,v.uv,0.).r);
}
@fragment fn composite(v:Vertex)->@location(0) vec4<f32> {
    let glass=textureSampleLevel(condensation,linear_sampler,v.uv,0.);
    let fog=clamp(glass.a,0.,1.);
    let blur=glass.rgb;
    var live=textureSampleLevel(dynamic_field,linear_sampler,v.uv,0.);
    // The cached beads lie under this frame's drops. Condensation beads need
    // fogged glass: they vanish where a drop wiped and return as it fogs
    // over. Left-behind beads show only in the cleared path.
    let bead=textureSampleLevel(beads,linear_sampler,v.uv,0.);
    if bead.w>0.001 {
        let beaded=smoothstep(0.45,0.95,fog);
        let left=(1.-smoothstep(0.2,0.7,fog))*step(0.001,p.style.z);
        let under=vec4(bead.xy,abs(bead.z),bead.w)*select(left,beaded,bead.z>=0.);
        live=vec4(live.xy,abs(live.z),live.w)+under*(1.-clamp(live.w,0.,1.));
    }
    // Most of the glass holds no water; skip the lens work there.
    var water=Water(vec2(0.),0.,0.,0.);
    var lod=0.;
    let strength=0.6+p.motion.w*7.;
    if live.w>0.002 {
        water=decode(live);
        // Read the mip that matches the lens's minification, so the view
        // through a moving drop neither sparkles nor thrashes the cache.
        let q=live.xy/max(live.w,1e-4);
        let dome=sqrt(1.-min(dot(q,q),1.));
        lod=log2(max(1.,(1.08+1.38*dome)*strength-1.))*water.clear;
    }
    // A drop is a lens: the sharp wallpaper, flipped and minified. Dry glass
    // reads the same texel unbent, so each pixel fetches the wallpaper once.
    let bend=water.bend*strength*vec2(1./p.frame.x,1.);
    let uv=clamp(v.uv+bend,vec2(0.001),vec2(0.999));
    let seen=textureSampleLevel(background,linear_sampler,uv,lod).rgb;
    // The blurred wallpaper's brightness lights the glass.
    let light=luminance(blur);
    // Fogged glass scatters: blurred, lighter and less saturated. Water
    // clears it.
    let frosted=mix(blur,vec3(light),0.25)*0.8+vec3(0.07+0.16*light);
    var rgb=mix(seen,frosted,fog*min(1.,p.motion.z*2.)*0.8*(1.-water.clear));
    // The wallpaper's own brightness lights the rim and highlight.
    let shine=min(water.shine*(0.55+0.9*light),0.72);
    rgb=mix(rgb*(1.-min(water.shade,0.72)),vec3(0.95,0.98,1.)*(0.35+0.85*light),shine);
    // A scene change fades toward the plain wallpaper, then back.
    if p.style.w<1. {
        let clean=textureSampleLevel(background,linear_sampler,v.uv,0.).rgb;
        rgb=mix(clean,rgb,p.style.w);
    }
    return vec4(rgb,1.);
}
// Downsampling is done only when the wallpaper changes, into cached mipmaps.
@fragment fn downsample(v:Vertex)->@location(0) vec4<f32> {
    return textureSampleLevel(background,linear_sampler,v.uv,0.);
}

// Three analytic snow layers, evaluated per output pixel so flakes move
// smoothly. A flake never gets smaller than about two output pixels across;
// smaller flakes fade instead, and every edge is antialiased.
fn snow(uv:vec2<f32>)->f32 {
    var alpha=0.;
    let pixel=max(p.tint.w,1e-5);
    for(var layer=0;layer<3;layer+=1) {
        let depth=f32(layer);
        let scale=22.+depth*18.;
        let speed=p.motion.x*0.14/(1.+depth*0.55);
        let t=p.frame.y*speed;
        let drift=sin(p.frame.y*p.motion.x*0.8+depth*2.7)*0.12;
        let q=uv*vec2(p.frame.x,1.)*scale-vec2(t*p.motion.y*scale*0.6+drift,t*scale)+depth*17.3;
        let cell=floor(q);let local=fract(q);
        let place=hash3(cell+depth*61.);
        if place.x>p.frame.z*0.8 {continue;}
        // One output pixel in cell units.
        let px=pixel*scale;
        // Most of a cell is empty sky: leave before the flake's own hash when
        // even the largest, most fluttered flake cannot reach this pixel.
        let reach=max(0.09*(0.45+p.frame.w*1.5),px)+0.5*px;
        let rest=vec2(0.35+place.y*0.3,0.25+place.z*0.5);
        if abs(local.x-rest.x)>0.1+reach || abs(local.y-rest.y)>1.15*reach {continue;}
        let look=hash3(cell+vec2(83.,depth*37.));
        let flutter=sin(p.frame.y*p.motion.x*(0.6+look.y)+look.z*6.283)*0.10;
        let center=rest+vec2(flutter,0.);
        let radius=(0.025+look.x*0.065)*(0.45+p.frame.w*1.5);
        // A flake too small to cover a pixel keeps its size on screen and
        // fades by area instead, so it cannot pop between pixels.
        let shown=max(radius,px);
        let presence=(radius*radius)/(shown*shown);
        let distance=length((local-center)/vec2(1.,0.85+fract(look.y*7.1+look.z*3.3)*0.3));
        let outer=shown+0.5*px;
        let inner=max(0.,min(shown*0.25,shown-1.5*px));
        let flake=(1.-smoothstep(inner,outer,distance))*presence*(0.85-depth*0.22);
        alpha+=flake;
    }
    return clamp(alpha,0.,1.)*p.atmosphere.x;
}
@fragment fn atmosphere(v:Vertex)->@location(0) f32 {
    if p.style.x==3. {return fog(v.uv);}
    if p.style.x==4. {return cloud_shadows(v.uv);}
    return 0.;
}

// Smooth value noise, three bounded octaves; no evolving textures or readback.
fn noise(q:vec2<f32>)->f32 {
    let cell=floor(q);let f=fract(q);let u=f*f*(3.-2.*f);
    return mix(mix(hash(cell),hash(cell+vec2(1.,0.)),u.x),mix(hash(cell+vec2(0.,1.)),hash(cell+vec2(1.,1.)),u.x),u.y);
}
fn billows(q:vec2<f32>)->f32 {
    return noise(q)*0.57+noise(q*2.03+vec2(13.7,7.1))*0.29+noise(q*4.11+vec2(37.9,19.3))*0.14;
}
fn fog(uv:vec2<f32>)->f32 {
    if p.atmosphere.x<=0. || p.atmosphere.y<=0. {return 0.;}
    let t=p.frame.y*p.motion.x*0.08;
    let q=uv*vec2(p.frame.x*2.4,5.8)-vec2(t*(0.15+p.motion.y*1.4),t*0.12);
    let body=billows(q+vec2(noise(q*0.5)*0.8,0.));
    let top=1.-p.atmosphere.y;
    let height=smoothstep(top,min(1.,top+0.35),uv.y);
    return height*p.atmosphere.x*(0.22+smoothstep(0.20,0.85,body)*0.78);
}

fn cloud_shadows(uv:vec2<f32>)->f32 {
    if p.frame.z<=0. || p.atmosphere.x<=0. {return 0.;}
    let t=p.frame.y*p.motion.x*0.06;
    let q=uv*vec2(p.frame.x*2.1,2.7)-vec2(t*(0.2+p.motion.y),t*0.15);
    let broad=noise(q);
    let detail=billows(q);
    let body=mix(detail,broad,p.atmosphere.z*0.7);
    let edge=0.025+p.atmosphere.z*0.28;
    let threshold=1.-p.frame.z;
    let cover=smoothstep(threshold-edge,threshold+edge,body);
    return cover*p.atmosphere.x*0.75;
}
