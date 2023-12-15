export * from './common/global'

declare namespace dataSource {
    function create(name: string, params: Array<string>): void
}

export function dataSourceCreate(name: string, params: Array<string>): void {
    dataSource.create(name, params)
}
